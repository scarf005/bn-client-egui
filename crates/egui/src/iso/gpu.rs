//! The 3D map on the GPU: the scene is drawn into a texture of its own, with a depth buffer egui's pass has
//! not, and copied into the egui pass over the map's rectangle.

use super::camera::{LIGHT, Viewport};
use super::geometry::{Instance, PageId};
use super::pages::Pages;
use super::scene::Scene;
use super::sky::SkyUniform;
use bytemuck::{Pod, Zeroable};
use eframe::egui;
use egui_wgpu::wgpu::{self, util::DeviceExt};
use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};
use glam::Mat4;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The side of the square depth map of the light's view, in texels, at most.
const SHADOW_SIZE: u32 = 4096;

/// What the camera gives the shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    light: [[f32; 4]; 4],
    right: [f32; 4],
    toward: [f32; 4],
    sun: [f32; 4],
}

impl CameraUniform {
    /// The camera of `v`, with the view of the light shadows are cast from.
    pub fn new(v: &Viewport, light: Mat4) -> Self {
        Self {
            view_proj: v.view_proj().to_cols_array_2d(),
            light: light.to_cols_array_2d(),
            right: v.right().extend(0.0).to_array(),
            toward: v.toward().extend(0.0).to_array(),
            sun: LIGHT.with_z(0.0).normalize().extend(0.0).to_array(),
        }
    }
}

/// What the last paint drew, for tests and measurements.
#[derive(Default)]
pub struct Stats {
    pub draws: AtomicU32,
    pub instances: AtomicU32,
}

/// One paint of the scene over `size` points of the window.
pub struct Paint {
    pub scene: Arc<Scene>,
    pub pages: Arc<Pages>,
    pub camera: CameraUniform,
    pub size: egui::Vec2,
    /// The format of the pass the scene is copied into.
    pub target: wgpu::TextureFormat,
    pub stats: Arc<Stats>,
    /// The sky behind the scene; black without one.
    pub sky: Option<SkyUniform>,
}

struct Offscreen {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    copy: wgpu::BindGroup,
}

struct Gpu {
    target: wgpu::TextureFormat,
    scene: wgpu::RenderPipeline,
    /// Draws the depth of the scene as the light sees it into `shadow_map`.
    shadow: wgpu::RenderPipeline,
    shadow_map: wgpu::TextureView,
    shadow_group: wgpu::BindGroup,
    blit: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    sky_buffer: wgpu::Buffer,
    sky_group: wgpu::BindGroup,
    camera: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    sheet_layout: wgpu::BindGroupLayout,
    copy_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sheets: HashMap<PageId, wgpu::BindGroup>,
    /// The pages the uploaded `sheets` came from; another tileset's pages replace them.
    pages: Option<Arc<Pages>>,
    instances: Option<wgpu::Buffer>,
    version: Option<u64>,
    /// The instances of each page: first and count.
    runs: Vec<(PageId, u32, u32)>,
    offscreen: Option<Offscreen>,
}

fn texture_layout(
    device: &wgpu::Device,
    label: &str,
    stages: wgpu::ShaderStages,
) -> wgpu::BindGroupLayout {
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stages,
        ty,
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            entry(
                0,
                wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
            ),
            entry(
                1,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ],
    })
}

impl Gpu {
    fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iso3d camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let sheet_layout =
            texture_layout(device, "iso3d sheet", wgpu::ShaderStages::VERTEX_FRAGMENT);
        let copy_layout = texture_layout(device, "iso3d copy", wgpu::ShaderStages::FRAGMENT);
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iso3d shadows"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let side = SHADOW_SIZE.min(device.limits().max_texture_dimension_2d);
        let shadow_map = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("iso3d shadows"),
                size: wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        // A point is lit where it lies no deeper in the light's view than what the light sees first.
        let shadowed = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("iso3d shadowed"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iso3d shadows"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_map),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadowed),
                },
            ],
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("iso3d camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iso3d camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        // Sprites are pixel art: a pixel is never blended with its neighbour in the sheet.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("iso3d pixels"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = |label, groups: &[Option<&wgpu::BindGroupLayout>]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: groups,
                immediate_size: 0,
            })
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("iso3d scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3,
            3 => Float32x2, 4 => Float32x2, 5 => Float32x2, 6 => Uint32x3
        ];
        let instances = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attributes,
        })];
        let shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iso3d shadow"),
            layout: Some(&layout(
                "iso3d shadow",
                &[Some(&camera_layout), Some(&sheet_layout)],
            )),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &instances,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                // A surface does not shadow itself where the light grazes it.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let scene = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iso3d scene"),
            layout: Some(&layout(
                "iso3d scene",
                &[
                    Some(&camera_layout),
                    Some(&sheet_layout),
                    Some(&shadow_layout),
                ],
            )),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &instances,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                // Equal depth goes to the later sprite of a square.
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sky_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iso3d sky"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let sky_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("iso3d sky"),
            size: std::mem::size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iso3d sky"),
            layout: &sky_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_buffer.as_entire_binding(),
            }],
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("iso3d sky"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });
        let sky = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iso3d sky"),
            layout: Some(&layout("iso3d sky", &[Some(&sky_layout)])),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            // Drawn first and behind everything, so the scene covers it wherever it draws.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let copy = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("iso3d copy"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iso3d copy"),
            layout: Some(&layout("iso3d copy", &[Some(&copy_layout)])),
            vertex: wgpu::VertexState {
                module: &copy,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &copy,
                entry_point: Some(if target.is_srgb() {
                    "fs_linear_framebuffer"
                } else {
                    "fs_main"
                }),
                compilation_options: Default::default(),
                targets: &[Some(target.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            target,
            scene,
            shadow,
            shadow_map,
            shadow_group,
            blit,
            sky,
            sky_buffer,
            sky_group,
            camera,
            camera_group,
            sheet_layout,
            copy_layout,
            sampler,
            sheets: HashMap::new(),
            pages: None,
            instances: None,
            version: None,
            runs: vec![],
            offscreen: None,
        }
    }

    /// The texture of a page, uploaded the first time it is drawn; none while the page is being decoded.
    fn sheet(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pages: &Pages,
        id: PageId,
    ) -> Option<&wgpu::BindGroup> {
        if !self.sheets.contains_key(&id) {
            let image = pages.get(id)?;
            let size = wgpu::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("iso3d page"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: COLOR,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                image.as_raw(),
            );
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("iso3d page"),
                layout: &self.sheet_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.sheets.insert(id, group);
        }
        self.sheets.get(&id)
    }

    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.offscreen.as_ref().is_some_and(|o| o.size == size) {
            return;
        }
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let texture = |label, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = texture(
            "iso3d color",
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = texture("iso3d depth", DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let copy = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iso3d copy"),
            layout: &self.copy_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.offscreen = Some(Offscreen {
            size,
            color,
            depth,
            copy,
        });
    }

    fn upload(&mut self, device: &wgpu::Device, scene: &Scene) {
        if self.version == Some(scene.version) {
            return;
        }
        let mut all =
            Vec::<Instance>::with_capacity(scene.batches.iter().map(|b| b.instances.len()).sum());
        self.runs.clear();
        for batch in &scene.batches {
            self.runs
                .push((batch.page, all.len() as u32, batch.instances.len() as u32));
            all.extend_from_slice(&batch.instances);
        }
        self.instances = (!all.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("iso3d instances"),
                contents: bytemuck::cast_slice(&all),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        self.version = Some(scene.version);
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        paint: &Paint,
        size: [u32; 2],
    ) {
        if !self
            .pages
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(p, &paint.pages))
        {
            self.sheets.clear();
            self.pages = Some(paint.pages.clone());
        }
        let limit = device.limits().max_texture_dimension_2d;
        self.resize(device, size.map(|n| n.min(limit)));
        self.upload(device, &paint.scene);
        queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&paint.camera));
        if let Some(sky) = &paint.sky {
            queue.write_buffer(&self.sky_buffer, 0, bytemuck::bytes_of(sky));
        }
        // Pages still being decoded are left out until they arrive.
        let runs = self.runs.clone();
        for (page, ..) in &runs {
            self.sheet(device, queue, &paint.pages, *page);
        }
        let Some(off) = &self.offscreen else {
            return;
        };
        if let Some(instances) = &self.instances {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("iso3d shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_vertex_buffer(0, instances.slice(..));
            for (page, first, count) in &runs {
                if let Some(sheet) = self.sheets.get(page) {
                    pass.set_bind_group(1, sheet, &[]);
                    pass.draw(0..4, *first..*first + *count);
                }
            }
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("iso3d scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &off.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &off.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if paint.sky.is_some() {
            pass.set_pipeline(&self.sky);
            pass.set_bind_group(0, &self.sky_group, &[]);
            pass.draw(0..3, 0..1);
        }
        let (mut draws, mut drawn) = (0, 0);
        if let Some(instances) = &self.instances {
            pass.set_pipeline(&self.scene);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_bind_group(2, &self.shadow_group, &[]);
            pass.set_vertex_buffer(0, instances.slice(..));
            for (page, first, count) in &runs {
                if let Some(sheet) = self.sheets.get(page) {
                    pass.set_bind_group(1, sheet, &[]);
                    pass.draw(0..4, *first..*first + *count);
                    (draws, drawn) = (draws + 1, drawn + count);
                }
            }
        }
        paint.stats.draws.store(draws, Ordering::Relaxed);
        paint.stats.instances.store(drawn, Ordering::Relaxed);
    }
}

impl CallbackTrait for Paint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources
            .get::<Gpu>()
            .is_none_or(|g| g.target != self.target)
        {
            resources.insert(Gpu::new(device, self.target));
        }
        let size = (self.size * screen.pixels_per_point)
            .round()
            .max(egui::Vec2::splat(1.0));
        if let Some(gpu) = resources.get_mut::<Gpu>() {
            gpu.draw(device, queue, encoder, self, [size.x as u32, size.y as u32]);
        }
        vec![]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(gpu) = resources.get::<Gpu>() else {
            return;
        };
        if let Some(off) = &gpu.offscreen {
            pass.set_pipeline(&gpu.blit);
            pass.set_bind_group(0, &off.copy, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

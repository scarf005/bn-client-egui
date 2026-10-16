//! The engine's sound cues on the default audio device, from the sound pack its hello names.

use bn_client_core::audio::{Cue, Output, Play, Player, SoundPack, Track};
use rodio::mixer::Mixer;
use rodio::source::ChannelVolume;
use rodio::{Decoder, Source};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// How often a fade or the music is looked at while one runs.
const STEP: Duration = Duration::from_millis(50);

/// A track fading out: its gain falls from `from` to nothing over `fade` after `since`.
struct Fading {
    voice: rodio::Player,
    from: f32,
    since: Instant,
    fade: Duration,
}

/// A channel's or the music's voice, with the direction its samples read as they play (`f32` bits, NaN for none).
struct Voice {
    player: rodio::Player,
    pan: Arc<AtomicU32>,
}

/// Left and right gains: equal power from the left at -1 to the right at 1, both full for no direction.
fn sides(pan: f32) -> (f32, f32) {
    if pan.is_nan() {
        return (1.0, 1.0);
    }
    let side = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (side.cos(), side.sin())
}

/// Plays files into a mixer: one voice per channel and for the music, and one per effect that lives until it ends.
pub struct Speakers {
    mixer: Mixer,
    tracks: HashMap<Track, Voice>,
    effects: Vec<rodio::Player>,
    fading: Vec<Fading>,
    /// `BN_SOUND_LOG`: every file started or track stopped, with the milliseconds since the speakers opened.
    log: Option<(std::fs::File, Instant)>,
}

fn decode(file: &Path) -> Option<Decoder<std::io::BufReader<std::fs::File>>> {
    Decoder::try_from(std::fs::File::open(file).ok()?).ok()
}

impl Speakers {
    pub fn new(mixer: Mixer) -> Self {
        let log = std::env::var_os("BN_SOUND_LOG")
            .and_then(|p| std::fs::File::create(p).ok())
            .map(|f| (f, Instant::now()));
        let mut speakers = Self {
            mixer,
            tracks: HashMap::new(),
            effects: vec![],
            fading: vec![],
            log,
        };
        speakers.note(format_args!("open"));
        speakers
    }

    fn note(&mut self, line: std::fmt::Arguments) {
        if let Some((file, opened)) = &mut self.log {
            let _ = writeln!(file, "{} {line}", opened.elapsed().as_millis());
        }
    }

    /// Lowers the fading tracks; true while one still fades.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.fading.retain(|f| {
            let done = now.saturating_duration_since(f.since).as_secs_f32()
                / f.fade.as_secs_f32().max(1e-3);
            if done >= 1.0 {
                f.voice.stop();
                return false;
            }
            f.voice.set_volume(f.from * (1.0 - done));
            true
        });
        !self.fading.is_empty()
    }
}

impl Output for Speakers {
    fn play(&mut self, play: Play) -> Option<Duration> {
        if play.track != Track::Effect {
            self.stop(&play.track, Duration::ZERO);
        }
        let Some(source) = decode(&play.file) else {
            self.note(format_args!("missing {}", play.file.display()));
            return None;
        };
        let length = source.total_duration();
        let source = source.buffered();
        let voice = rodio::Player::connect_new(&self.mixer);
        voice.set_volume(play.gain);
        let pan = Arc::new(AtomicU32::new(play.pan.unwrap_or(f32::NAN).to_bits()));
        // A channel can turn later; an effect keeps its direction and the music has none.
        let steered = matches!(play.track, Track::Channel(_)) || play.pan.is_some();
        let shaped = |s: Box<dyn Source + Send>, fade: bool| -> Box<dyn Source + Send> {
            let s: Box<dyn Source + Send> = if steered {
                let pan = pan.clone();
                let (left, right) = sides(f32::from_bits(pan.load(Ordering::Relaxed)));
                Box::new(ChannelVolume::new(s, vec![left, right]).periodic_access(
                    Duration::from_millis(20),
                    move |s| {
                        let (left, right) = sides(f32::from_bits(pan.load(Ordering::Relaxed)));
                        s.set_volume(0, left);
                        s.set_volume(1, right);
                    },
                ))
            } else {
                s
            };
            if !fade || play.fade_in.is_zero() {
                s
            } else {
                Box::new(s.fade_in(play.fade_in))
            }
        };
        match play.loops {
            // Only the first time fades in, as the native player's fade does.
            Some(more) => {
                for time in 0..=more {
                    voice.append(shaped(Box::new(source.clone()), time == 0));
                }
            }
            None => voice.append(shaped(Box::new(source.repeat_infinite()), true)),
        }
        self.note(format_args!(
            "play {:?} {} gain={:.3} pan={:?} loops={:?}",
            play.track,
            play.file.display(),
            play.gain,
            play.pan,
            play.loops
        ));
        match play.track {
            Track::Effect => {
                self.effects.retain(|e| !e.empty());
                self.effects.push(voice);
            }
            track => {
                self.tracks.insert(track, Voice { player: voice, pan });
            }
        }
        length
    }

    fn stop(&mut self, track: &Track, fade: Duration) {
        let voices = match track {
            Track::Effect => std::mem::take(&mut self.effects),
            track => self
                .tracks
                .remove(track)
                .map(|v| v.player)
                .into_iter()
                .collect(),
        };
        if voices.is_empty() {
            return;
        }
        self.note(format_args!("stop {track:?} fade={}", fade.as_millis()));
        for voice in voices {
            if fade.is_zero() {
                voice.stop();
            } else {
                self.fading.push(Fading {
                    from: voice.volume(),
                    voice,
                    since: Instant::now(),
                    fade,
                });
            }
        }
    }

    fn set_gain(&mut self, track: &Track, gain: f32) {
        if let Some(voice) = self.tracks.get(track) {
            voice.player.set_volume(gain);
        }
    }

    fn set_pan(&mut self, track: &Track, pan: Option<f32>) {
        if let Some(voice) = self.tracks.get(track) {
            voice
                .pan
                .store(pan.unwrap_or(f32::NAN).to_bits(), Ordering::Relaxed);
        }
    }

    fn playing(&self, track: &Track) -> bool {
        self.tracks.get(track).is_some_and(|v| !v.player.empty())
    }
}

/// The audio device and what plays on it, opened when the engine first names a sound pack.
#[derive(Default)]
pub struct Audio {
    open: Option<Open>,
    /// The pack the speakers play, or the one that failed to open, so a failure is not retried every frame.
    tried: Option<PathBuf>,
}

struct Open {
    _device: rodio::MixerDeviceSink,
    speakers: Speakers,
    player: Player,
}

impl Audio {
    /// Plays the cues; true while a fade or the music needs another look soon.
    pub fn update(&mut self, soundpack: Option<PathBuf>, cues: Vec<Cue>) -> Option<Duration> {
        if soundpack != self.tried {
            self.tried = soundpack.clone();
            self.open = soundpack.and_then(|dir| {
                let pack = SoundPack::load(&dir).ok()?;
                let mut device = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
                device.log_on_drop(false);
                Some(Open {
                    speakers: Speakers::new(device.mixer().clone()),
                    player: Player::new(pack),
                    _device: device,
                })
            });
        }
        let open = self.open.as_mut()?;
        let now = Instant::now();
        for cue in cues {
            open.player.apply(cue, &mut open.speakers, now);
        }
        open.player.tick(&mut open.speakers);
        let fading = open.speakers.tick(now);
        (fading || open.player.active()).then_some(STEP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;

    /// A one-second 440 Hz mono tone as 16-bit PCM WAV.
    fn tone(path: &Path) {
        let rate = 8000u32;
        let samples = (0..rate)
            .map(|i| {
                ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12000.0) as i16
            })
            .collect::<Vec<_>>();
        let data = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<_>>();
        let mut wav = vec![];
        wav.extend(b"RIFF");
        wav.extend((36 + data.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 2).to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((data.len() as u32).to_le_bytes());
        wav.extend(data);
        std::fs::write(path, wav).unwrap();
    }

    fn loudest(source: &mut impl Iterator<Item = f32>, count: usize) -> f32 {
        source.take(count).fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn a_played_file_is_decoded_and_mixed_at_its_gain() {
        let dir = std::env::temp_dir().join(format!("bn-speakers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tone.wav");
        tone(&file);
        let (mixer, mut out) =
            rodio::mixer::mixer(NonZero::new(2).unwrap(), NonZero::new(8000).unwrap());
        let mut speakers = Speakers::new(mixer);
        let length = speakers.play(Play {
            track: Track::Effect,
            file: file.clone(),
            gain: 0.5,
            pan: None,
            loops: Some(0),
            fade_in: Duration::ZERO,
        });
        assert_eq!(length, Some(Duration::from_secs(1)));
        // 12000 of 32768 at half gain.
        let level = loudest(&mut out, 4000);
        assert!((level - 0.183).abs() < 0.01, "{level}");
        // A file the pack names but does not have plays nothing.
        assert_eq!(
            speakers.play(Play {
                track: Track::Music,
                file: dir.join("missing.ogg"),
                gain: 1.0,
                pan: None,
                loops: Some(0),
                fade_in: Duration::ZERO
            }),
            None
        );
        assert!(!speakers.playing(&Track::Music));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The sound packs are Ogg Vorbis, whose decoder starts with an empty span.
    #[test]
    fn an_ogg_file_plays_once_repeats_a_set_number_of_times_and_loops() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/tone.ogg");
        let (mixer, mut out) =
            rodio::mixer::mixer(NonZero::new(2).unwrap(), NonZero::new(48000).unwrap());
        let mut speakers = Speakers::new(mixer);
        let play = |track: Track, loops: Option<u32>| Play {
            track,
            file: file.clone(),
            gain: 1.0,
            pan: None,
            loops,
            fade_in: Duration::ZERO,
        };
        // A second of stereo samples.
        let second = 96000;
        // The one-second tone (about 0.09 of full scale), twice: sounding in its first and second seconds, not after.
        speakers.play(play(Track::Music, Some(1)));
        assert!(loudest(&mut out, second) > 0.05);
        assert!(speakers.playing(&Track::Music));
        assert!(loudest(&mut out, second) > 0.05);
        assert!(loudest(&mut out.by_ref().skip(second / 10), second) < 1e-3);
        assert!(!speakers.playing(&Track::Music));
        // A track repeated until stopped still sounds after many times its length.
        let track = Track::Channel("ambience".into());
        speakers.play(play(track.clone(), None));
        assert!(loudest(&mut out.by_ref().skip(5 * second), second) > 0.05);
        assert!(speakers.playing(&track));
    }

    #[test]
    fn a_channel_pans_repeats_and_fades_out() {
        let dir = std::env::temp_dir().join(format!("bn-channel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tone.wav");
        tone(&file);
        let (mixer, mut out) =
            rodio::mixer::mixer(NonZero::new(2).unwrap(), NonZero::new(8000).unwrap());
        let mut speakers = Speakers::new(mixer);
        let track = Track::Channel("radio".into());
        speakers.play(Play {
            track: track.clone(),
            file,
            gain: 1.0,
            pan: Some(1.0),
            loops: None,
            fade_in: Duration::ZERO,
        });
        // Fully to the right: the left samples are silent.
        let frames = out.by_ref().take(4000).collect::<Vec<_>>();
        let left = frames.iter().step_by(2).fold(0.0f32, |m, s| m.max(s.abs()));
        let right = frames
            .iter()
            .skip(1)
            .step_by(2)
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(left < 1e-3, "{left}");
        assert!(right > 0.3, "{right}");
        // It repeats past its one second.
        assert!(loudest(&mut out.by_ref().skip(16000), 2000) > 0.3);
        assert!(speakers.playing(&track));
        // It turns to the left while it plays.
        speakers.set_pan(&track, Some(-1.0));
        let frames = out.by_ref().skip(2000).take(4000).collect::<Vec<_>>();
        let right = frames
            .iter()
            .skip(1)
            .step_by(2)
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(right < 1e-3, "{right}");
        speakers.stop(&track, Duration::from_millis(100));
        assert!(!speakers.playing(&track));
        assert!(speakers.tick(Instant::now()));
        assert!(!speakers.tick(Instant::now() + Duration::from_secs(1)));
        // Stopping the effects fades out every one that plays.
        for _ in 0..2 {
            speakers.play(Play {
                track: Track::Effect,
                file: dir.join("tone.wav"),
                gain: 1.0,
                pan: None,
                loops: Some(3),
                fade_in: Duration::ZERO,
            });
        }
        speakers.stop(&Track::Effect, Duration::from_millis(100));
        assert_eq!(speakers.fading.len(), 2);
        assert!(speakers.effects.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

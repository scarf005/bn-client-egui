//! Plays what the engine's sound player is asked to play (`sound.played`, `sound.channel`, `music.changed`) from the
//! sound pack the hello names, choosing files the way the native player does. Output is a trait, so the choice of
//! file, gain, direction and timing is tested without a device.

use serde::Deserialize;
use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The full gain in the engine's mixer units.
const FULL: f32 = 128.0;

/// One file of a `sound_effect` or a playlist entry, with its own volume (0-100).
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub file: PathBuf,
    pub volume: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playlist {
    pub shuffle: bool,
    pub entries: Vec<Sample>,
}

/// The `sound_effect`s and `playlist`s of every JSON file under a sound pack's folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundPack {
    effects: HashMap<(String, String), Vec<Sample>>,
    playlists: HashMap<String, Playlist>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry {
    SoundEffect {
        id: String,
        #[serde(default = "default_variant")]
        variant: String,
        #[serde(default = "full_volume")]
        volume: u32,
        files: Vec<String>,
    },
    Playlist {
        playlists: Vec<PlaylistEntry>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct PlaylistEntry {
    id: String,
    #[serde(default)]
    shuffle: bool,
    files: Vec<PlaylistFile>,
}

#[derive(Deserialize)]
struct PlaylistFile {
    file: String,
    volume: u32,
}

fn default_variant() -> String {
    "default".to_string()
}

fn full_volume() -> u32 {
    100
}

impl SoundPack {
    /// Reads every `.json` file under `dir`, as the native loader does; a file that is not sound pack JSON is skipped.
    pub fn load(dir: &Path) -> std::io::Result<Self> {
        let mut pack = Self::default();
        let mut pending = vec![dir.to_path_buf()];
        let mut files = vec![];
        while let Some(at) = pending.pop() {
            for entry in std::fs::read_dir(&at)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|e| e == "json") {
                    files.push(path);
                }
            }
        }
        files.sort();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let entries = match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(serde_json::Value::Array(items)) => items,
                Ok(item) => vec![item],
                Err(_) => continue,
            };
            for entry in entries {
                pack.add(dir, entry);
            }
        }
        Ok(pack)
    }

    fn add(&mut self, dir: &Path, entry: serde_json::Value) {
        match serde_json::from_value::<Entry>(entry) {
            Ok(Entry::SoundEffect {
                id,
                variant,
                volume,
                files,
            }) => self
                .effects
                .entry((id, variant))
                .or_default()
                .extend(files.into_iter().map(|f| Sample {
                    file: dir.join(f),
                    volume,
                })),
            Ok(Entry::Playlist { playlists }) => {
                for list in playlists {
                    let entries = list.files.into_iter().map(|f| Sample {
                        file: dir.join(f.file),
                        volume: f.volume,
                    });
                    self.playlists.insert(
                        list.id,
                        Playlist {
                            shuffle: list.shuffle,
                            entries: entries.collect(),
                        },
                    );
                }
            }
            Ok(Entry::Other) | Err(_) => {}
        }
    }

    /// The files of an effect's variant, else of its `default` variant, as the native player falls back.
    pub fn effect(&self, id: &str, variant: &str) -> Option<&[Sample]> {
        let files = |v: &str| {
            self.effects
                .get(&(id.to_string(), v.to_string()))
                .filter(|f| !f.is_empty())
        };
        files(variant)
            .or_else(|| files("default"))
            .map(Vec::as_slice)
    }

    pub fn playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.get(id).filter(|p| !p.entries.is_empty())
    }
}

/// A `sound.played`, `sound.channel` or `music.changed` event's data.
#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    Effect(EffectCue),
    Channel(ChannelCue),
    Music(MusicCue),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct EffectCue {
    pub id: String,
    pub variant: String,
    pub volume: u32,
    #[serde(default)]
    pub angle: Option<i32>,
    pub stacks: bool,
    /// How long after the last effect with a wait this one may play; none for no wait.
    #[serde(default)]
    pub wait_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ChannelCue {
    pub channel: String,
    /// What the channel plays now; none when it stops.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub volume: u32,
    /// The gain a volume change set, in mixer units, in place of `volume` and the pack's volume.
    #[serde(default)]
    pub gain: Option<u32>,
    #[serde(default)]
    pub angle: Option<i32>,
    #[serde(default = "forever")]
    pub loops: i32,
    pub fade_ms: u64,
}

fn forever() -> i32 {
    -1
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct MusicCue {
    /// None when the music stops.
    #[serde(default)]
    pub playlist: Option<String>,
    #[serde(default)]
    pub volume: u32,
}

impl Cue {
    /// The cue of an event of one of the sound types; none for any other event.
    pub fn of_event(kind: &str, data: &serde_json::Value) -> Option<Self> {
        let data = data.clone();
        match kind {
            "sound.played" => serde_json::from_value(data).ok().map(Cue::Effect),
            "sound.channel" => serde_json::from_value(data).ok().map(Cue::Channel),
            "music.changed" => serde_json::from_value(data).ok().map(Cue::Music),
            _ => None,
        }
    }
}

/// The channel that stops every channel and effect.
const ANY: &str = "any";

/// Where a sound plays: a one-shot effect, a reserved channel, or the music.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Track {
    Effect,
    Channel(String),
    Music,
}

/// One file to start.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    pub track: Track,
    pub file: PathBuf,
    /// 0-1 times the file's own level.
    pub gain: f32,
    /// -1 left to 1 right; none for no direction.
    pub pan: Option<f32>,
    /// How often the file repeats after the first time; none until the track stops.
    pub loops: Option<u32>,
    pub fade_in: Duration,
}

/// What actually makes the sound.
pub trait Output {
    /// Starts a file, replacing what a channel or the music played; gives its length when it is known.
    fn play(&mut self, play: Play) -> Option<Duration>;
    /// Fades out what a channel or the music plays, or every effect for `Track::Effect`.
    fn stop(&mut self, track: &Track, fade: Duration);
    /// The new gain of what a channel or the music plays.
    fn set_gain(&mut self, track: &Track, gain: f32);
    /// The new direction of what a channel plays; none for no direction.
    fn set_pan(&mut self, track: &Track, pan: Option<f32>);
    /// Whether a channel or the music still plays.
    fn playing(&self, track: &Track) -> bool;
}

/// What a channel was last told to play.
#[derive(Clone, Debug)]
struct Channel {
    id: String,
    variant: String,
    gain: f32,
    angle: Option<i32>,
    repeats: bool,
}

#[derive(Clone, Debug)]
struct Music {
    playlist: String,
    volume: u32,
    order: Vec<usize>,
    at: usize,
    /// When the song playing ends, where its length is known.
    ends: Option<Instant>,
}

/// How soon a song of unknown length is looked at again for its end.
const SONG_POLL: Duration = Duration::from_secs(1);

/// Turns cues into `Output` calls the way the native player turns calls into mixer tracks.
pub struct Player {
    pack: SoundPack,
    channels: HashMap<String, Channel>,
    music: Option<Music>,
    /// When an effect asked not to stack may play again, by its id and variant.
    busy_until: HashMap<(String, String), Instant>,
    random: RandomState,
    draws: u64,
}

fn gain(sample_volume: u32, volume: u32) -> f32 {
    (sample_volume.min(100) as f32 / 100.0 * volume as f32 / FULL).clamp(0.0, 1.0)
}

/// A channel's gain: a volume change's own, else its start's as an effect's.
fn channel_gain(cue: &ChannelCue, sample_volume: u32) -> f32 {
    cue.gain.map_or_else(
        || gain(sample_volume, cue.volume),
        |g| (g as f32 / FULL).clamp(0.0, 1.0),
    )
}

/// The native mixer places a sound at (sin, cos) of its angle, so its side is the sine.
fn pan(angle: Option<i32>) -> Option<f32> {
    angle.map(|a| (a as f32).to_radians().sin())
}

impl Player {
    pub fn new(pack: SoundPack) -> Self {
        Self {
            pack,
            channels: HashMap::new(),
            music: None,
            busy_until: HashMap::new(),
            random: RandomState::new(),
            draws: 0,
        }
    }

    /// One of `count` choices; audio variation is presentation, not game state.
    fn pick(&mut self, count: usize) -> usize {
        self.draws += 1;
        (self.random.hash_one(self.draws) % count as u64) as usize
    }

    fn sample(&mut self, id: &str, variant: &str) -> Option<Sample> {
        let count = self.pack.effect(id, variant)?.len();
        let index = self.pick(count);
        self.pack
            .effect(id, variant)
            .map(|files| files[index].clone())
    }

    pub fn apply(&mut self, cue: Cue, out: &mut impl Output, now: Instant) {
        match cue {
            Cue::Effect(cue) => self.effect(cue, out, now),
            Cue::Channel(cue) => self.channel(cue, out),
            Cue::Music(cue) => self.change_music(cue, out, now),
        }
    }

    fn effect(&mut self, cue: EffectCue, out: &mut impl Output, now: Instant) {
        let key = (cue.id.clone(), cue.variant.clone());
        if !cue.stacks && self.busy_until.get(&key).is_some_and(|until| now < *until) {
            return;
        }
        let Some(sample) = self.sample(&cue.id, &cue.variant) else {
            return;
        };
        let length = out.play(Play {
            track: Track::Effect,
            gain: gain(sample.volume, cue.volume),
            file: sample.file,
            pan: pan(cue.angle),
            loops: Some(0),
            fade_in: Duration::ZERO,
        });
        if !cue.stacks {
            self.busy_until
                .insert(key, now + length.unwrap_or_default());
        }
    }

    fn channel(&mut self, cue: ChannelCue, out: &mut impl Output) {
        let track = Track::Channel(cue.channel.clone());
        let fade = Duration::from_millis(cue.fade_ms);
        // A sound that ends by itself is over once its track is.
        if self
            .channels
            .get(&cue.channel)
            .is_some_and(|c| !c.repeats && !out.playing(&track))
        {
            self.channels.remove(&cue.channel);
        }
        let (Some(id), Some(variant)) = (cue.id.clone(), cue.variant.clone()) else {
            if cue.channel == ANY {
                for (name, _) in self.channels.drain() {
                    out.stop(&Track::Channel(name), fade);
                }
                out.stop(&Track::Effect, fade);
            } else if self.channels.remove(&cue.channel).is_some() {
                out.stop(&track, fade);
            }
            return;
        };
        if let Some(current) = self.channels.get_mut(&cue.channel) {
            // The same sound plays on at its new level and direction; anything else waits until the channel is free.
            if current.id == id && current.variant == variant {
                let level = cue.gain.map_or(current.gain, |_| channel_gain(&cue, 0));
                if level != current.gain {
                    current.gain = level;
                    out.set_gain(&track, level);
                }
                if cue.angle != current.angle {
                    current.angle = cue.angle;
                    out.set_pan(&track, pan(cue.angle));
                }
            }
            return;
        }
        let Some(sample) = self.sample(&id, &variant) else {
            return;
        };
        let level = channel_gain(&cue, sample.volume);
        out.play(Play {
            track,
            file: sample.file,
            gain: level,
            pan: pan(cue.angle),
            loops: u32::try_from(cue.loops).ok(),
            fade_in: fade,
        });
        self.channels.insert(
            cue.channel,
            Channel {
                id,
                variant,
                gain: level,
                angle: cue.angle,
                repeats: cue.loops < 0,
            },
        );
    }

    fn change_music(&mut self, cue: MusicCue, out: &mut impl Output, now: Instant) {
        let Some(playlist) = cue.playlist else {
            if self.music.take().is_some() {
                out.stop(&Track::Music, Duration::ZERO);
            }
            return;
        };
        if let Some(music) = &mut self.music
            && music.playlist == playlist
        {
            music.volume = cue.volume;
            if let Some(entry) = self
                .pack
                .playlist(&playlist)
                .map(|p| &p.entries[music.order[music.at]])
            {
                out.set_gain(&Track::Music, gain(entry.volume, cue.volume));
            }
            return;
        }
        let Some(list) = self.pack.playlist(&playlist) else {
            self.music = None;
            out.stop(&Track::Music, Duration::ZERO);
            return;
        };
        let mut order = (0..list.entries.len()).collect::<Vec<_>>();
        if list.shuffle {
            for i in (1..order.len()).rev() {
                let j = self.pick(i + 1);
                order.swap(i, j);
            }
        }
        self.music = Some(Music {
            playlist,
            volume: cue.volume,
            order,
            at: 0,
            ends: None,
        });
        self.play_music(out, now);
    }

    fn play_music(&mut self, out: &mut impl Output, now: Instant) {
        let Some(music) = &self.music else {
            return;
        };
        let Some(entry) = self
            .pack
            .playlist(&music.playlist)
            .map(|p| p.entries[music.order[music.at]].clone())
        else {
            return;
        };
        let length = out.play(Play {
            track: Track::Music,
            file: entry.file,
            gain: gain(entry.volume, music.volume),
            pan: None,
            loops: Some(0),
            fade_in: Duration::ZERO,
        });
        if let Some(music) = &mut self.music {
            music.ends = length.map(|l| now + l);
        }
        // A song that does not play stops the music, as natively, instead of skipping through the list.
        if !out.playing(&Track::Music) {
            self.music = None;
        }
    }

    /// Goes on to the next song when one ends, wrapping around as the native playlist does.
    pub fn tick(&mut self, out: &mut impl Output, now: Instant) {
        let Some(music) = &mut self.music else {
            return;
        };
        if out.playing(&Track::Music) {
            return;
        }
        music.at = (music.at + 1) % music.order.len();
        self.play_music(out, now);
    }

    /// How soon the music may need its next song without a new cue: when the song ends, or a poll for a song of
    /// unknown length; none without music.
    pub fn next_check(&self, now: Instant) -> Option<Duration> {
        let music = self.music.as_ref()?;
        Some(
            music
                .ends
                .map_or(SONG_POLL, |end| end.saturating_duration_since(now)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what was asked; a track plays until stopped or until a test ends it.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<String>,
        plays: Vec<Play>,
        live: std::collections::HashSet<Track>,
        length: Option<Duration>,
        /// Every file fails to play.
        silent: bool,
    }

    impl Output for Recorder {
        fn play(&mut self, play: Play) -> Option<Duration> {
            self.calls
                .push(format!("play {:?} {}", play.track, play.file.display()));
            if play.track != Track::Effect && !self.silent {
                self.live.insert(play.track.clone());
            }
            self.plays.push(play);
            self.length
        }
        fn stop(&mut self, track: &Track, fade: Duration) {
            self.calls
                .push(format!("stop {track:?} {}", fade.as_millis()));
            self.live.remove(track);
        }
        fn set_gain(&mut self, track: &Track, gain: f32) {
            self.calls.push(format!("gain {track:?} {gain}"));
        }
        fn set_pan(&mut self, track: &Track, pan: Option<f32>) {
            self.calls.push(format!("pan {track:?} {pan:?}"));
        }
        fn playing(&self, track: &Track) -> bool {
            self.live.contains(track)
        }
    }

    fn pack_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bn-pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("more")).unwrap();
        std::fs::write(
            dir.join("soundset.json"),
            r#"[
              { "type": "sound_effect", "id": "open_door", "volume": 50, "files": [ "door.ogg" ] },
              { "type": "sound_effect", "id": "fire_gun", "variant": "glock_19", "files": [ "glock.ogg" ] },
              { "type": "sound_effect_preload", "preload": [ { "id": "fire_gun", "variant": "all" } ] },
              { "type": "sound_effect", "id": "environment", "variant": "daytime", "volume": 80, "files": [ "day.ogg" ] }
            ]"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("more/musicset.json"),
            r#"{ "type": "playlist", "playlists": [
              { "id": "title", "files": [ { "file": "a.ogg", "volume": 100 }, { "file": "b.ogg", "volume": 50 } ] } ] }"#,
        )
        .unwrap();
        std::fs::write(dir.join("notes.json"), "not json").unwrap();
        dir
    }

    fn played(id: &str, variant: &str, volume: u32, stacks: bool) -> Cue {
        Cue::Effect(EffectCue {
            id: id.into(),
            variant: variant.into(),
            volume,
            angle: None,
            stacks,
            wait_ms: None,
        })
    }

    fn paced(variant: &str, wait_ms: u64) -> Cue {
        let Cue::Effect(mut cue) = played("plmove", variant, 100, false) else {
            unreachable!()
        };
        cue.wait_ms = Some(wait_ms);
        Cue::Effect(cue)
    }

    fn channel(id: Option<&str>, volume: u32, loops: i32, fade_ms: u64) -> Cue {
        Cue::Channel(ChannelCue {
            channel: "daytime_outdoors_env".into(),
            id: id.map(|_| "environment".into()),
            variant: id.map(str::to_string),
            volume,
            gain: None,
            angle: None,
            loops,
            fade_ms,
        })
    }

    /// A volume or direction change of the daytime ambience.
    fn changed(gain: Option<u32>, angle: Option<i32>) -> Cue {
        let Cue::Channel(mut cue) = channel(Some("daytime"), 128, -1, 0) else {
            unreachable!()
        };
        cue.gain = gain;
        cue.angle = angle;
        Cue::Channel(cue)
    }

    #[test]
    fn a_pack_loads_every_json_file_under_its_folder() {
        let dir = pack_dir("load");
        let pack = SoundPack::load(&dir).unwrap();
        let door = pack.effect("open_door", "default").unwrap();
        assert_eq!(
            door,
            [Sample {
                file: dir.join("door.ogg"),
                volume: 50
            }]
        );
        assert_eq!(pack.effect("fire_gun", "glock_19").unwrap()[0].volume, 100);
        let title = pack.playlist("title").unwrap();
        assert_eq!(title.entries.len(), 2);
        assert_eq!(title.entries[1].file, dir.join("b.ogg"));
        assert!(SoundPack::load(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_variant_falls_back_to_default_and_a_missing_effect_to_nothing() {
        let dir = pack_dir("fallback");
        let pack = SoundPack::load(&dir).unwrap();
        // The door has only a default variant; a door type's variant plays it.
        assert_eq!(
            pack.effect("open_door", "t_door_c").unwrap()[0].file,
            dir.join("door.ogg")
        );
        // A gun with no variant of its own and no default plays nothing, as natively.
        assert!(pack.effect("fire_gun", "m4a1").is_none());
        assert!(pack.effect("no_such", "default").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_effect_plays_its_file_at_the_pack_and_engine_volume() {
        let dir = pack_dir("effect");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder::default();
        player.apply(
            played("open_door", "t_door_c", 64, true),
            &mut out,
            Instant::now(),
        );
        let mut aimed = played("fire_gun", "glock_19", 128, true);
        if let Cue::Effect(e) = &mut aimed {
            e.angle = Some(90);
        }
        player.apply(aimed, &mut out, Instant::now());
        player.apply(
            played("fire_gun", "m4a1", 128, true),
            &mut out,
            Instant::now(),
        );
        assert_eq!(out.plays.len(), 2);
        assert_eq!(out.plays[0].file, dir.join("door.ogg"));
        // 50 % of the file at half the mixer's gain.
        assert_eq!(out.plays[0].gain, 0.25);
        assert_eq!(out.plays[0].pan, None);
        assert_eq!(out.plays[1].gain, 1.0);
        assert!((out.plays[1].pan.unwrap() - 1.0).abs() < 1e-6);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_effect_that_does_not_stack_waits_for_its_length() {
        let dir = pack_dir("stack");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder {
            length: Some(Duration::from_secs(1)),
            ..Default::default()
        };
        let start = Instant::now();
        player.apply(played("open_door", "default", 100, false), &mut out, start);
        player.apply(
            played("open_door", "default", 100, false),
            &mut out,
            start + Duration::from_millis(500),
        );
        // A stacking one plays over it.
        player.apply(
            played("open_door", "default", 100, true),
            &mut out,
            start + Duration::from_millis(600),
        );
        player.apply(
            played("open_door", "default", 100, false),
            &mut out,
            start + Duration::from_secs(1),
        );
        assert_eq!(out.plays.len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_effect_with_a_wait_plays_only_once_the_last_one_with_a_wait_is_that_old() {
        let dir = pack_dir("paced");
        std::fs::write(
            dir.join("steps.json"),
            r#"[ { "type": "sound_effect", "id": "plmove", "variant": "walk_barefoot", "files": [ "step.ogg" ] },
                 { "type": "sound_effect", "id": "plmove", "variant": "clear_obstacle", "files": [ "bump.ogg" ] } ]"#,
        )
        .unwrap();
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        player.apply(paced("walk_barefoot", 400), &mut out, at(0));
        player.apply(paced("walk_barefoot", 400), &mut out, at(399));
        player.apply(paced("walk_barefoot", 400), &mut out, at(400));
        // An obstacle plays at once and delays the next step.
        player.apply(paced("clear_obstacle", 0), &mut out, at(500));
        player.apply(paced("walk_barefoot", 400), &mut out, at(850));
        player.apply(paced("walk_barefoot", 400), &mut out, at(900));
        let files = out
            .plays
            .iter()
            .map(|p| p.file.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(files, ["step.ogg", "step.ogg", "bump.ogg", "step.ogg"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_channel_starts_changes_and_stops() {
        let dir = pack_dir("channel");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder::default();
        let now = Instant::now();
        player.apply(channel(Some("daytime"), 128, -1, 1000), &mut out, now);
        // A volume change sets the gain itself, past the pack's volume, as the native one does.
        player.apply(changed(Some(64), None), &mut out, now);
        player.apply(changed(Some(64), None), &mut out, now);
        player.apply(changed(Some(64), Some(90)), &mut out, now);
        player.apply(channel(None, 0, -1, 500), &mut out, now);
        player.apply(channel(None, 0, -1, 500), &mut out, now);
        let track = Track::Channel("daytime_outdoors_env".into());
        assert_eq!(
            out.calls,
            [
                format!("play {track:?} {}", dir.join("day.ogg").display()),
                format!("gain {track:?} 0.5"),
                format!("pan {track:?} Some(1.0)"),
                format!("stop {track:?} 500"),
            ]
        );
        assert_eq!(out.plays[0].loops, None);
        assert_eq!(out.plays[0].fade_in, Duration::from_secs(1));
        assert_eq!(out.plays[0].gain, 0.8);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stopping_any_channel_stops_every_channel_and_effect() {
        let dir = pack_dir("any");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder::default();
        let now = Instant::now();
        player.apply(channel(Some("daytime"), 128, -1, 0), &mut out, now);
        let Cue::Channel(mut any) = channel(None, 0, -1, 300) else {
            unreachable!()
        };
        any.channel = "any".into();
        player.apply(Cue::Channel(any), &mut out, now);
        let track = Track::Channel("daytime_outdoors_env".into());
        assert_eq!(
            out.calls[1..],
            [format!("stop {track:?} 300"), "stop Effect 300".to_string()]
        );
        // The channel is free for its next start.
        player.apply(channel(Some("daytime"), 128, -1, 0), &mut out, now);
        assert_eq!(out.plays.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_song_that_does_not_play_stops_the_music() {
        let dir = pack_dir("song");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder {
            silent: true,
            ..Default::default()
        };
        player.apply(
            Cue::Music(MusicCue {
                playlist: Some("title".into()),
                volume: 128,
            }),
            &mut out,
            Instant::now(),
        );
        player.tick(&mut out, Instant::now());
        assert_eq!(out.plays.len(), 1);
        assert_eq!(player.next_check(Instant::now()), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn music_is_looked_at_again_when_its_song_ends_not_every_frame() {
        let dir = pack_dir("idle");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let now = Instant::now();
        let title = Cue::Music(MusicCue {
            playlist: Some("title".into()),
            volume: 128,
        });
        let mut known = Recorder {
            length: Some(Duration::from_secs(90)),
            ..Default::default()
        };
        player.apply(title.clone(), &mut known, now);
        assert_eq!(
            player.next_check(now + Duration::from_secs(30)),
            Some(Duration::from_secs(60))
        );
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        player.apply(title, &mut Recorder::default(), now);
        assert_eq!(player.next_check(now), Some(SONG_POLL));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_channel_ignores_a_start_while_its_sound_still_plays() {
        let dir = pack_dir("busy");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder::default();
        let now = Instant::now();
        let track = Track::Channel("daytime_outdoors_env".into());
        player.apply(channel(Some("daytime"), 128, 0, 0), &mut out, now);
        player.apply(channel(Some("daytime"), 128, 0, 0), &mut out, now);
        assert_eq!(out.plays.len(), 1);
        assert_eq!(out.plays[0].loops, Some(0));
        // Once it ended by itself, the next start plays it again.
        out.live.remove(&track);
        player.apply(channel(Some("daytime"), 128, 0, 0), &mut out, now);
        assert_eq!(out.plays.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn music_plays_its_playlist_in_order_and_wraps() {
        let dir = pack_dir("music");
        let mut player = Player::new(SoundPack::load(&dir).unwrap());
        let mut out = Recorder::default();
        let title = |volume| {
            Cue::Music(MusicCue {
                playlist: Some("title".into()),
                volume,
            })
        };
        player.apply(title(128), &mut out, Instant::now());
        player.apply(title(64), &mut out, Instant::now());
        player.tick(&mut out, Instant::now());
        out.live.remove(&Track::Music);
        player.tick(&mut out, Instant::now());
        out.live.remove(&Track::Music);
        player.tick(&mut out, Instant::now());
        let files = out
            .plays
            .iter()
            .map(|p| p.file.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(files, ["a.ogg", "b.ogg", "a.ogg"]);
        assert_eq!(out.plays[1].gain, 0.25);
        assert!(out.calls.contains(&"gain Music 0.5".to_string()));
        player.apply(
            Cue::Music(MusicCue {
                playlist: None,
                volume: 0,
            }),
            &mut out,
            Instant::now(),
        );
        assert_eq!(player.next_check(Instant::now()), None);
        assert_eq!(out.calls.last().unwrap(), "stop Music 0");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cues_decode_from_their_events() {
        let data = serde_json::json!({"id": "fire_gun", "variant": "glock_19", "volume": 90, "angle": 270, "stacks": true, "wait_ms": 80});
        assert_eq!(
            Cue::of_event("sound.played", &data),
            Some(Cue::Effect(EffectCue {
                id: "fire_gun".into(),
                variant: "glock_19".into(),
                volume: 90,
                angle: Some(270),
                stacks: true,
                wait_ms: Some(80),
            }))
        );
        let stop = serde_json::json!({"channel": "radio", "fade_ms": 300});
        assert_eq!(
            Cue::of_event("sound.channel", &stop),
            Some(Cue::Channel(ChannelCue {
                channel: "radio".into(),
                id: None,
                variant: None,
                volume: 0,
                gain: None,
                angle: None,
                loops: -1,
                fade_ms: 300,
            }))
        );
        assert_eq!(
            Cue::of_event("music.changed", &serde_json::json!({})),
            Some(Cue::Music(MusicCue {
                playlist: None,
                volume: 0
            }))
        );
        assert_eq!(Cue::of_event("sound.heard", &data), None);
    }
}

//! # Native Shell for KeyInSight
//!
//! Thinnest possible desktop shim: everything platform-generic (winit
//! window and event loop, wgpu surface, input forwarding, frame painting)
//! lives in `demo_wgpu::native_shell`. This file contributes only what is
//! genuinely specific to KeyInSight on desktop: the [`KeyInSightPlatform`]
//! implementation (file-backed storage under the OS app-data directory;
//! MIDI via midir and audio out via cpal land here next — see
//! `docs/platform-substitutions.md`) and the per-frame engine tick.

mod audio;

use std::path::PathBuf;
use std::rc::Rc;

use keyinsight_core::audio::AudioOut;
use keyinsight_core::persistence::Storage;
use keyinsight_core::{build_keyinsight_app, KeyInSightPlatform, UiFonts};

/// File-backed storage in the platform app-data directory (the port of
/// `AppDatabase.onDisk()`'s Application Support path).
struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    fn in_app_data() -> Option<Self> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| {
                    let mut p = PathBuf::from(home);
                    p.push(".local");
                    p.push("share");
                    p
                })
            })?;
        let dir = base.join("KeyInSight");
        std::fs::create_dir_all(&dir).ok()?;
        Some(Self {
            path: dir.join("keyinsight.json"),
        })
    }
}

impl Storage for FileStorage {
    fn load(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }

    fn save(&self, contents: &str) {
        // Persistence failures never take down the training loop (the
        // Swift app logged and continued the same way).
        if let Err(err) = std::fs::write(&self.path, contents) {
            eprintln!("KeyInSight: persistence unavailable ({err}) — continuing without it");
        }
    }
}

/// Desktop implementation of the platform capability surface.
struct NativePlatform;

impl KeyInSightPlatform for NativePlatform {
    fn storage(&self) -> Option<Box<dyn Storage>> {
        FileStorage::in_app_data().map(|s| Box::new(s) as Box<dyn Storage>)
    }

    /// Metronome clicks + Hear It playback through the default output
    /// device (silent fallback when none exists).
    fn audio(&self) -> Rc<dyn AudioOut> {
        Rc::new(audio::CpalAudioOut::new())
    }

    /// Real microphone capture: the mic input source detects played
    /// notes with the Goertzel bank. Opens the device lazily on first
    /// use.
    fn mic(&self) -> Option<Rc<dyn keyinsight_core::input::MicSource>> {
        Some(Rc::new(audio::CpalMicSource::new()))
    }

    fn supports_musicxml_import(&self) -> bool {
        true
    }

    /// Native file picker for the Library sheet's Import (the
    /// `NSOpenPanel` in `LibrarySheet.swift`). `rfd` blocks the event
    /// loop while open, same as the Swift `runModal()`.
    fn open_musicxml(&self, on_file: Box<dyn FnOnce(Vec<u8>, String)>) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MusicXML", &["musicxml", "xml"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read(&path) {
            Ok(data) => {
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Imported".to_string());
                on_file(data, name);
            }
            Err(err) => eprintln!("KeyInSight: couldn't read {}: {err}", path.display()),
        }
    }
}

fn main() {
    // Headless audio diagnostic: play a C-major arpeggio + two clicks
    // through the real output path and exit (`keyinsight-native --audio-smoke`).
    if std::env::args().any(|arg| arg == "--audio-smoke") {
        audio_smoke();
        return;
    }
    // Loopback diagnostic: play a chord through the speakers and detect
    // it on the default microphone (`keyinsight-native --mic-smoke`).
    if std::env::args().any(|arg| arg == "--mic-smoke") {
        mic_smoke();
        return;
    }

    let (app, handles) = build_keyinsight_app(UiFonts::bundled(), NativePlatform);

    demo_wgpu::native_shell::run(
        demo_wgpu::NativeShellConfig {
            title: "KeyInSight",
            logical_size: (1180.0, 640.0),
        },
        app,
        // Advance the engine every painted frame (input queue, deferred
        // actions, metronome sweep).
        move || handles.tick(),
    );
}

fn mic_smoke() {
    use keyinsight_core::audio::MidiFileEncoder;
    use keyinsight_core::core::InputBackend;
    use keyinsight_core::input::{MicBackend, MicSource};
    use keyinsight_core::score::{Exercise, NoteDuration, ScoreNote};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Tee: copies drained samples so the smoke can report raw ratios.
    struct TeeMic {
        inner: audio::CpalMicSource,
        window: RefCell<Vec<f32>>,
    }
    impl MicSource for TeeMic {
        fn start(&self) -> bool {
            self.inner.start()
        }
        fn stop(&self) {
            self.inner.stop()
        }
        fn sample_rate(&self) -> f64 {
            self.inner.sample_rate()
        }
        fn drain(&self, out: &mut Vec<f32>) {
            let before = out.len();
            self.inner.drain(out);
            let mut window = self.window.borrow_mut();
            window.extend_from_slice(&out[before..]);
            let excess = window.len().saturating_sub(4096);
            window.drain(..excess);
        }
    }

    let mic: Rc<TeeMic> = Rc::new(TeeMic {
        inner: audio::CpalMicSource::new(),
        window: RefCell::new(Vec::new()),
    });
    let mut backend = MicBackend::new(Rc::clone(&mic) as Rc<dyn MicSource>);
    let detected: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&detected);
    backend.set_on_event(Some(Box::new(move |event| {
        if event.kind == keyinsight_core::core::NoteEventKind::On && event.confidence >= 1.0 {
            sink.borrow_mut().push(event.midi);
        }
    })));
    backend.start();

    // The capture device can take a second to deliver its first samples;
    // don't start the chord until the mic is actually flowing.
    let warmup = std::time::Instant::now();
    let mut probe = Vec::new();
    while probe.len() < 4096 && warmup.elapsed().as_secs_f64() < 5.0 {
        mic.drain(&mut probe);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    println!(
        "mic-smoke: mic flowing after {:.2}s ({} samples)",
        warmup.elapsed().as_secs_f64(),
        probe.len()
    );

    let out = audio::CpalAudioOut::new();
    // Three back-to-back chords keep fresh attacks coming.
    let chord_notes = |_: ()| {
        vec![
            ScoreNote::note(60, NoteDuration::Whole),
            ScoreNote::note(64, NoteDuration::Whole).with_chord(true),
            ScoreNote::note(67, NoteDuration::Whole).with_chord(true),
        ]
    };
    let mut notes = Vec::new();
    for _ in 0..3 {
        notes.extend(chord_notes(()));
    }
    let chord = Exercise::new(notes, 4);
    let accepted = out.play_smf(&MidiFileEncoder::encode(&chord, 90.0, 0));
    println!("mic-smoke: playing C-major chords (speakers on?) accepted = {accepted}");

    let start = std::time::Instant::now();
    let mut peak_level = 0.0f64;
    let mut peak_ratio = [0.0f64; 3];
    let mut sub_contrast = [f64::INFINITY; 3];
    while start.elapsed().as_secs_f64() < 8.0 {
        backend.process(keyinsight_core::host_now(), &[60, 64, 67]);
        peak_level = peak_level.max(backend.level());
        {
            let window = mic.window.borrow();
            if window.len() >= 4096 {
                let rate = MicSource::sample_rate(&*mic);
                for (i, midi) in [60u8, 64, 67].iter().enumerate() {
                    peak_ratio[i] = peak_ratio[i].max(
                        keyinsight_core::audio::goertzel::candidate_ratio(&window, rate, *midi),
                    );
                    sub_contrast[i] = sub_contrast[i].min(
                        keyinsight_core::audio::goertzel::sub_octave_contrast(
                            &window, rate, *midi,
                        ),
                    );
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    backend.stop();
    let mut hits = detected.borrow().clone();
    hits.sort_unstable();
    hits.dedup();
    println!(
        "mic-smoke: detected {hits:?} (want [60, 64, 67]); peak mic level {peak_level:.4}; peak ratios C/E/G = {peak_ratio:.3?}; min sub-octave contrast = {sub_contrast:.2?}"
    );
}

fn audio_smoke() {
    use keyinsight_core::audio::MidiFileEncoder;
    use keyinsight_core::score::{Exercise, NoteDuration, ScoreNote};

    let out = audio::CpalAudioOut::new();
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Quarter),
            ScoreNote::note(67, NoteDuration::Quarter),
            ScoreNote::note(72, NoteDuration::Half),
        ],
        4,
    );
    let smf = MidiFileEncoder::encode(&exercise, 120.0, 0);
    let playing = out.play_smf(&smf);
    let now = keyinsight_core::host_now();
    out.play_click(now + 0.5, true);
    out.play_click(now + 1.0, false);
    println!("audio-smoke: play_smf accepted = {playing}");
    std::thread::sleep(std::time::Duration::from_millis(3500));
    println!("audio-smoke: done");
}

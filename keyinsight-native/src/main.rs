//! # Native Shell for KeyInSight
//!
//! Thinnest possible desktop shim: everything platform-generic (winit
//! window and event loop, wgpu surface, input forwarding, frame painting)
//! lives in `demo_wgpu::native_shell`. This file contributes only what is
//! genuinely specific to KeyInSight on desktop: the [`KeyInSightPlatform`]
//! implementation (file-backed storage under the OS app-data directory,
//! MIDI via midir, audio out + mic via cpal — see
//! `docs/platform-substitutions.md`) and the per-frame engine tick.
//!
//! Dev / diagnostic launch flags:
//!
//! - `--audio-smoke`, `--mic-smoke`, `--midi-smoke` — headless device checks
//! - `--demo` — the scripted, headless playthrough
//! - `--piece <slug>` — open straight into a bundled piece
//! - `--survival` — start a survival run
//! - `--library`, `--progress`, `--about`, `--profile`, `--calibration` —
//!   open that sheet at launch
//! - `--screenshot <path>` — paint a few settle frames, write the window to
//!   `<path>` as a PNG (physical pixels: 2x on Retina), then exit

mod audio;
mod midi;

use std::cell::RefCell;
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
        // Windows: %APPDATA%; macOS: ~/Library/Application Support (the
        // exact directory the Swift app's `AppDatabase.onDisk()` used);
        // elsewhere: XDG ~/.local/share.
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| {
                    let mut p = PathBuf::from(home);
                    if cfg!(target_os = "macos") {
                        p.push("Library");
                        p.push("Application Support");
                    } else {
                        p.push(".local");
                        p.push("share");
                    }
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
struct NativePlatform {
    /// The held display-awake assertion (`DisplaySleepGuard.swift`);
    /// dropping it releases.
    display_guard: RefCell<Option<keepawake::KeepAwake>>,
}

impl NativePlatform {
    fn new() -> Self {
        Self {
            display_guard: RefCell::new(None),
        }
    }
}

impl KeyInSightPlatform for NativePlatform {
    fn storage(&self) -> Option<Box<dyn Storage>> {
        FileStorage::in_app_data().map(|s| Box::new(s) as Box<dyn Storage>)
    }

    /// Metronome clicks + Hear It playback through the default output
    /// device (silent fallback when none exists).
    fn audio(&self) -> Rc<dyn AudioOut> {
        Rc::new(audio::CpalAudioOut::new())
    }

    /// Real MIDI input over midir: every input port, hot-plug aware.
    fn midi(&self) -> Option<Rc<dyn keyinsight_core::input::MidiPorts>> {
        Some(Rc::new(midi::MidirPorts::new()))
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

    /// Display-sleep guard: `SetThreadExecutionState` on Windows, an
    /// IOKit power assertion on macOS, a D-Bus inhibitor on Linux.
    fn set_display_awake(&self, active: bool) {
        let mut guard = self.display_guard.borrow_mut();
        if active == guard.is_some() {
            return;
        }
        if active {
            match keepawake::Builder::default()
                .display(true)
                .reason("KeyInSight exercise in progress")
                .app_name("KeyInSight")
                .app_reverse_domain("com.keyinsight.app")
                .create()
            {
                Ok(handle) => *guard = Some(handle),
                Err(err) => eprintln!("KeyInSight: display-awake unavailable ({err})"),
            }
        } else {
            *guard = None;
        }
    }
}

/// Frames painted before `--screenshot` captures, so fonts, layout and
/// start-up animations have settled.
const SCREENSHOT_SETTLE_FRAMES: u32 = 6;

/// What `--screenshot` asked for, as read off the command line.
#[derive(Debug, PartialEq, Eq)]
enum ScreenshotArg<'a> {
    /// The flag is absent: run the app windowed, as usual.
    Absent,
    /// `--screenshot <path>`: capture to `<path>` and exit.
    Path(&'a str),
    /// `--screenshot` with nothing usable behind it (end of the command
    /// line, or another flag). Running windowed here would hang a
    /// capture script that waits for the file, so this is an error.
    MissingPath,
}

/// Read the `--screenshot` argument out of `args` (which includes argv[0],
/// like [`std::env::args`]).
///
/// The value must be a real path: anything starting with `--` is the next
/// flag, not the destination.
fn screenshot_arg(args: &[String]) -> ScreenshotArg<'_> {
    let Some(i) = args.iter().position(|arg| arg == "--screenshot") else {
        return ScreenshotArg::Absent;
    };
    match args.get(i + 1) {
        Some(path) if !path.starts_with("--") => ScreenshotArg::Path(path),
        _ => ScreenshotArg::MissingPath,
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
    // MIDI diagnostic: list input ports and print parsed note events
    // for ~10 s (`keyinsight-native --midi-smoke`).
    if std::env::args().any(|arg| arg == "--midi-smoke") {
        midi::midi_smoke();
        return;
    }
    // The scripted playthrough (the Swift `--demo`): headless, no window,
    // no audio device needed; exits with the demo's code (0 = every act
    // verified).
    if std::env::args().any(|arg| arg == "--demo") {
        std::process::exit(demo());
    }

    let (app, handles) = build_keyinsight_app(UiFonts::bundled(), NativePlatform::new());
    // Dev convenience (the Swift `--piece <slug>` launch hook): open
    // straight into a bundled piece.
    let args: Vec<String> = std::env::args().collect();
    if let Some(slug) = args
        .iter()
        .position(|arg| arg == "--piece")
        .and_then(|i| args.get(i + 1))
    {
        if let Some(piece) = keyinsight_core::score::RepertoireLibrary::bundled()
            .into_iter()
            .find(|p| &p.slug == slug)
        {
            handles.engine.borrow_mut().start_piece(piece);
        }
    }
    // Dev convenience (the Swift `--survival` launch hook): start a
    // survival run straight away.
    if std::env::args().any(|arg| arg == "--survival") {
        handles.engine.borrow_mut().enter_survival();
    }
    // Dev convenience (the Swift `--library` launch hook, plus the same
    // hook for the other sheets): open one straight away.
    if std::env::args().any(|arg| arg == "--library") {
        handles.open_library();
    }
    if std::env::args().any(|arg| arg == "--progress") {
        handles.open_progress();
    }
    if std::env::args().any(|arg| arg == "--about") {
        handles.open_about();
    }
    if std::env::args().any(|arg| arg == "--profile") {
        handles.open_profile();
    }
    if std::env::args().any(|arg| arg == "--calibration") {
        handles.open_calibration();
    }

    // The Swift TrainingView's minWidth 1180 / minHeight 520.
    let mut config = demo_wgpu::NativeShellConfig::new("KeyInSight", (1180.0, 640.0))
        .with_min_size(1180.0, 520.0);
    // Deterministic capture (`--screenshot <path>`): paint a few settle
    // frames, write the window as a PNG, and exit.
    match screenshot_arg(&args) {
        ScreenshotArg::Path(path) => {
            config = config.with_screenshot(path, SCREENSHOT_SETTLE_FRAMES);
        }
        // Fail loudly: a capture script that waits for the PNG would
        // otherwise sit in front of an ordinary, never-exiting window.
        ScreenshotArg::MissingPath => {
            eprintln!("keyinsight-native: --screenshot needs a file path");
            eprintln!("usage: keyinsight-native --screenshot <path.png> [other flags]");
            std::process::exit(2);
        }
        ScreenshotArg::Absent => {}
    }

    demo_wgpu::native_shell::run(
        config,
        app,
        // Advance the engine every painted frame (input queue, deferred
        // actions, metronome sweep).
        move || handles.tick(),
    );
}

/// `keyinsight-native --demo`: the whole training loop, scripted, against
/// a throwaway in-memory database and the scripted clock — the engine
/// `engine:` trace and every `demo:` line go to stdout.
fn demo() -> i32 {
    use keyinsight_core::engine::{headless_demo_engine, run_demo};

    let (mut engine, clock) = headless_demo_engine();
    run_demo(&mut engine, clock)
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

#[cfg(test)]
mod tests {
    use super::{screenshot_arg, ScreenshotArg};

    fn args(items: &[&str]) -> Vec<String> {
        std::iter::once("keyinsight-native")
            .chain(items.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn no_screenshot_flag_runs_windowed() {
        assert_eq!(screenshot_arg(&args(&[])), ScreenshotArg::Absent);
        assert_eq!(screenshot_arg(&args(&["--library"])), ScreenshotArg::Absent);
    }

    #[test]
    fn screenshot_takes_the_path_that_follows_it() {
        assert_eq!(
            screenshot_arg(&args(&["--screenshot", "shot.png"])),
            ScreenshotArg::Path("shot.png")
        );
        assert_eq!(
            screenshot_arg(&args(&["--screenshot", "/tmp/a b/shot.png", "--library"])),
            ScreenshotArg::Path("/tmp/a b/shot.png")
        );
    }

    /// `--screenshot` at the end of the command line has no destination.
    #[test]
    fn screenshot_without_a_path_is_an_error() {
        assert_eq!(
            screenshot_arg(&args(&["--screenshot"])),
            ScreenshotArg::MissingPath
        );
    }

    /// The next flag is not a path — capturing to a file called
    /// `--library` is never what the caller meant.
    #[test]
    fn a_flag_after_screenshot_is_not_a_path() {
        assert_eq!(
            screenshot_arg(&args(&["--screenshot", "--library"])),
            ScreenshotArg::MissingPath
        );
    }
}

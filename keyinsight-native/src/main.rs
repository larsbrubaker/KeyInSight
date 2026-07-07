//! # Native Shell for KeyInSight
//!
//! Thinnest possible desktop shim: everything platform-generic (winit
//! window and event loop, wgpu surface, input forwarding, frame painting)
//! lives in `demo_wgpu::native_shell`. This file contributes only what is
//! genuinely specific to KeyInSight on desktop: the [`KeyInSightPlatform`]
//! implementation (file-backed storage under the OS app-data directory;
//! MIDI via midir and audio out via cpal land here next — see
//! `docs/platform-substitutions.md`) and the per-frame engine tick.

use std::path::PathBuf;

use keyinsight_core::persistence::Storage;
use keyinsight_core::{build_keyinsight_app, load_default_font, KeyInSightPlatform};

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
}

fn main() {
    let (app, handles) = build_keyinsight_app(load_default_font(), NativePlatform);

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

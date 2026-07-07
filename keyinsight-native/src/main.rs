//! # Native Shell for KeyInSight
//!
//! Thinnest possible desktop shim: everything platform-generic (winit
//! window and event loop, wgpu surface, input forwarding, frame painting)
//! lives in `demo_wgpu::native_shell`. This file contributes only what is genuinely
//! specific to KeyInSight on desktop: the [`KeyInSightPlatform`]
//! implementation (MIDI via midir, audio out via cpal, and file-backed
//! storage as those modules are ported — see
//! `docs/platform-substitutions.md`).

use keyinsight_core::{build_keyinsight_app, load_default_font, KeyInSightPlatform};

/// Desktop implementation of the platform capability surface.
struct NativePlatform;

impl KeyInSightPlatform for NativePlatform {}

fn main() {
    let app = build_keyinsight_app(load_default_font(), NativePlatform);

    demo_wgpu::native_shell::run(
        demo_wgpu::NativeShellConfig {
            title: "KeyInSight",
            logical_size: (1200.0, 800.0),
        },
        app,
        // Per-frame tick. The session engine's clock feeds from here once
        // the Engine module is ported.
        || {},
    );
}

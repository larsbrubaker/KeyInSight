// Browser bootstrap for KeyInSight's single Rust/agg-gui app.
//
// The Rust wasm module (demo-wgpu's `web_shell`) owns everything
// platform-generic the moment it loads: canvas sizing, the frame loop,
// pointer / wheel / keyboard / clipboard input. This file owns only loading
// the wasm module itself. Browser capabilities the port needs later (Web
// MIDI access, WebAudio unlock-on-gesture, getUserMedia) get their glue
// here, forwarding into Rust via wasm-bindgen exports — never visible UI.
// See docs/architecture.md.

// Build stamp injected by vite (see vite.config.ts `define`) — appended to
// the pkg/ asset URLs so a new deploy always busts the browser cache.
declare const __BUILD_ID__: string;

// wasm-pack --no-typescript does not emit .d.ts files; we reference the
// generated module structurally instead.
type WasmModule = {
  default: (url?: string | URL | { module_or_path: string | URL }) => Promise<unknown>;
  port_version: () => string;
};

const canvas = document.getElementById("keyinsight-canvas") as HTMLCanvasElement;

function showBootError(err: unknown): void {
  console.error("keyinsight: failed to boot wasm app", err);
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return;
  }
  canvas.width = Math.max(1, canvas.clientWidth || window.innerWidth);
  canvas.height = Math.max(1, canvas.clientHeight || window.innerHeight);
  ctx.fillStyle = "#101018";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#f2f2f7";
  ctx.font = "20px sans-serif";
  ctx.fillText("KeyInSight failed to load.", 24, 48);
  ctx.font = "14px sans-serif";
  ctx.fillText("Check that wasm-pack output exists at demo/public/pkg.", 24, 78);
  ctx.fillText(String((err as Error)?.message ?? err ?? ""), 24, 102);
}

async function boot(): Promise<void> {
  // Resolve against `document.baseURI` so the URL is correct both locally
  // and on GitHub Pages under the /KeyInSight/ sub-path.
  const v = `?v=${__BUILD_ID__}`;
  const url = new URL(`pkg/keyinsight_wasm.js${v}`, document.baseURI).href;
  const mod = (await import(/* @vite-ignore */ url)) as WasmModule;
  const wasmUrl = new URL(`pkg/keyinsight_wasm_bg.wasm${v}`, document.baseURI).href;
  // Module init runs the Rust `#[wasm_bindgen(start)]`, which boots the
  // whole shell (input, frame loop, rendering).
  await mod.default({ module_or_path: wasmUrl });
  console.log(`keyinsight: port version ${mod.port_version()}`);
}

void boot().catch(showBootError);

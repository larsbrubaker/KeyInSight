import { defineConfig } from "vite";

// GitHub Pages serves the demo at
// https://larsbrubaker.github.io/KeyInSight/
// so all asset paths must be prefixed accordingly. `./` works both there
// and locally under `vite dev`.
export default defineConfig({
  base: "./",
  // Stamped into the bundle and appended to the wasm-pack asset URLs in
  // main.ts. The pkg/ files are served with stable (unhashed) names, so
  // without this browsers keep serving a stale cached wasm long after a
  // deploy.
  define: {
    __BUILD_ID__: JSON.stringify(Date.now().toString(36)),
  },
  server: { host: true },
  preview: { host: true },
});

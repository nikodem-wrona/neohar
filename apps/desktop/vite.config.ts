import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// The dev server serves the page to the Tauri window (`mise run app:dev`). The port is fixed
// because `tauri.conf.json` names it. The packages of the workspace live above this directory.
// Lightning CSS (a dependency of Vite) reads the styles and lowers them for Safari 17: the
// popover API needs it (macOS 14).
export default defineConfig({
  plugins: [react()],
  css: { transformer: 'lightningcss', lightningcss: { targets: { safari: 17 << 16 } } },
  clearScreen: false,
  server: {
    host: 'localhost',
    port: 1420,
    strictPort: true,
    fs: { allow: ['../..'] },
  },
  build: { target: 'safari17', cssMinify: 'lightningcss', sourcemap: true },
});

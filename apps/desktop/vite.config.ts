import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// The dev server serves the page to the Tauri window (`mise run app:dev`). The port is fixed
// because `tauri.conf.json` names it. The packages of the workspace live above this directory.
// Lightning CSS (a dependency of Vite) reads the styles and lowers them for Safari 17, the WebKit
// of macOS 14. Tailwind CSS makes the classes of the shadcn/ui components, and
// `@/` is the source of the UI package, as the shadcn/ui CLI writes it.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': fileURLToPath(new URL('../../packages/ui/src', import.meta.url)) } },
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

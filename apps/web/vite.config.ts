import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// The phone app. `bun run build` writes `target/web-dist`, and the build of `nh-web` puts
// those files into the binary (`NEOHAR_WEB_DIST`). The service worker keeps the name `sw.js`, so
// that its scope is `/`. No source maps: the files go into `nh`, and the phone has no debugger.
const root = fileURLToPath(new URL('.', import.meta.url));

// Lightning CSS lowers the styles of the chrome for Safari 16 of the phone.
export default defineConfig({
  root,
  plugins: [react()],
  css: { transformer: 'lightningcss', lightningcss: { targets: { safari: 16 << 16 } } },
  clearScreen: false,
  build: {
    outDir: fileURLToPath(new URL('../../target/web-dist', import.meta.url)),
    emptyOutDir: true,
    target: 'safari16',
    cssMinify: 'lightningcss',
    sourcemap: false,
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL('index.html', import.meta.url)),
        sw: fileURLToPath(new URL('src/sw.ts', import.meta.url)),
      },
      output: {
        entryFileNames: (chunk) => (chunk.name === 'sw' ? 'sw.js' : 'assets/[name]-[hash].js'),
      },
    },
  },
});

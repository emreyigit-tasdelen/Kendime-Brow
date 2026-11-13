import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import { resolve } from 'node:path';

// Every top-level folder is one kb:// host: strip/ → kb://ui/, newtab/ → kb://newtab/.
// Relative base: kb://ui/../assets/x.js normalizes to kb://ui/assets/x.js, which the shell
// maps to dist/assets/x.js (see crates/kb-shell/src/scheme.rs).
export default defineConfig({
  plugins: [solid()],
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    modulePreload: { polyfill: false }, // the polyfill is an inline script, blocked by our CSP
    rollupOptions: {
      input: {
        ui: resolve(import.meta.dirname, 'strip/index.html'),
        newtab: resolve(import.meta.dirname, 'newtab/index.html'),
        settings: resolve(import.meta.dirname, 'settings/index.html'),
        downloads: resolve(import.meta.dirname, 'downloads/index.html'),
        tasks: resolve(import.meta.dirname, 'tasks/index.html'),
        pdf: resolve(import.meta.dirname, 'pdf/index.html'),
        office: resolve(import.meta.dirname, 'office/index.html'),
        media: resolve(import.meta.dirname, 'media/index.html'),
        tools: resolve(import.meta.dirname, 'tools/index.html'),
        term: resolve(import.meta.dirname, 'term/index.html'),
        vault: resolve(import.meta.dirname, 'vault/index.html'),
        files: resolve(import.meta.dirname, 'files/index.html'),
        connections: resolve(import.meta.dirname, 'connections/index.html'),
        net: resolve(import.meta.dirname, 'net/index.html'),
        logs: resolve(import.meta.dirname, 'logs/index.html'),
        scripts: resolve(import.meta.dirname, 'scripts/index.html'),
        localfiles: resolve(import.meta.dirname, 'localfiles/index.html'),
      },
    },
  },
});

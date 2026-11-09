import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';
export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  plugins: [react()],
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true, sourcemap: false, assetsInlineLimit: 0 },
  server: { host: '127.0.0.1', port: 1430, strictPort: true, proxy: { '/v1/rooms': { target: process.env.SSC_WEB_API_ORIGIN || 'http://127.0.0.1:8080' } } },
});

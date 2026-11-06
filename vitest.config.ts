import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  test: {
    include: ['src/**/*.{test,spec}.{ts,tsx,js,jsx}'],
    environment: 'jsdom', setupFiles: ['./src/test/setup.ts'], restoreMocks: true, clearMocks: true,
  },
});

import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri expects a fixed port and fails if it isn't available.
export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  // Component tests mount Svelte, which needs its browser build.
  resolve: mode === 'test' ? { conditions: ['browser'] } : undefined,
  test: {
    include: ['src/**/*.test.ts'],
  },
}));

import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        '**/resources/runtime/**',
        '**/.runtime-cache/**',
        '**/src-tauri/**',
      ],
    },
  },
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { target: 'es2022' },
});

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const isWeb = process.env.BUILD_TARGET === 'web';

export default defineConfig({
  plugins: [react()],
  base: isWeb ? '/projects/read-for-speed/' : '/',
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    outDir: isWeb ? 'dist-web' : 'dist',
    target: isWeb ? 'esnext' : (process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13'),
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});

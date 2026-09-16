import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

export default defineConfig(({ mode }) => {
  // vite.config.ts does not read .env automatically, loadEnv must be called explicitly
  const env = loadEnv(mode, process.cwd(), "");

  return {
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      }
    },
    server: {
      host: "0.0.0.0",
      port: 5173,
      allowedHosts: env.VITE_DEV_ALLOWED_HOST
        ? [env.VITE_DEV_ALLOWED_HOST]
        : undefined,
      proxy: {
        '/api': {
          target: 'http://backend:3000',
          changeOrigin: true,
        }
      }
    },
  };
});

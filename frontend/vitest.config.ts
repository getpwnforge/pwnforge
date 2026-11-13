import { defineConfig, mergeConfig } from 'vitest/config'
import viteConfig from './vite.config.ts'

// The test settings live here rather than in vite.config.ts: the production
// build never loads this file, so nothing of vitest reaches it, and the app's
// own configuration (aliases, plugins) is reused instead of copied.
export default defineConfig((configEnv) =>
  mergeConfig(
    viteConfig(configEnv),
    defineConfig({
      test: {
        environment: 'jsdom',
        include: ['src/**/*.test.{ts,tsx}'],
        setupFiles: ['./src/test/setup.ts'],
        // Spies, stubbed globals and environment variables are undone after
        // every test, so none leaks into the next.
        restoreMocks: true,
        unstubGlobals: true,
        unstubEnvs: true,
      },
    }),
  ),
)

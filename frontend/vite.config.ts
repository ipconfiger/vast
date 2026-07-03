import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), 'VITE_')

  return {
    plugins: [tailwindcss(), react()],
    server: {
      port: Number(env.VITE_DEV_PORT) || 5173,
      proxy: {
        '/api': {
          target: env.VITE_BACKEND_URL || 'http://localhost:3000',
          changeOrigin: true,
        },
        '/ws': {
          target: (env.VITE_BACKEND_URL || 'http://localhost:3000').replace(/^http/, 'ws'),
          ws: true,
        },
      },
    },
    build: {
      outDir: 'dist',
    },
  }
})

import path from "path"
import { defineConfig } from "vite"
import solidPlugin from "vite-plugin-solid"

export default defineConfig({
  resolve: {
    alias: {
      "~": path.resolve(__dirname, "src"),
      "solid-icons": path.resolve(__dirname, "node_modules/solid-icons"),
    },
  },
  plugins: [solidPlugin()],
  base: "/",
  server: {
    host: "127.0.0.1",
    proxy: {
      "/api": {
        target: "http://localhost:5244",
        changeOrigin: true,
      },
    },
  },
})

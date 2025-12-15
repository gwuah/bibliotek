import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  root: "static",
  build: {
    outDir: "../dist",
  },
  server: {
    proxy: {
      "/books": {
        target: "http://localhost:5678",
        changeOrigin: true,
      },
      "/upload": {
        target: "http://localhost:5678",
        changeOrigin: true,
      },
      "/metadata": {
        target: "http://localhost:5678",
        changeOrigin: true,
      },
      "/authors": {
        target: "http://localhost:5678",
        changeOrigin: true,
      },
      "/tags": {
        target: "http://localhost:5678",
        changeOrigin: true,
      },
      "/categories": {
        target: "http://localhost:5678",
        changeOrigin: true,
      },
    },
  },
});

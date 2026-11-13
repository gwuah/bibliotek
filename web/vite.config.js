import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // pdf.js ships modern syntax; every browser that runs it understands ES2022.
    target: "es2022",
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:6579",
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.js"],
  },
});

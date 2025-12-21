import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// API proxy config - only proxy fetch/XHR requests, not page navigation
const apiProxy = {
  target: "http://localhost:5678",
  changeOrigin: true,
  // Only proxy if it's an API request (not a page navigation)
  bypass: (req) => {
    // If it's a page navigation (accepts HTML), don't proxy - let SPA handle it
    if (req.headers.accept?.includes("text/html")) {
      return "/index.html";
    }
  },
};

export default defineConfig({
  plugins: [react()],
  root: "static",
  build: {
    outDir: "../dist",
  },
  appType: "spa",
  server: {
    proxy: {
      "/books": apiProxy,
      "/upload": apiProxy,
      "/metadata": apiProxy,
      "/authors": apiProxy,
      "/tags": apiProxy,
      "/categories": apiProxy,
      "/commonplace": apiProxy,
      "/light": apiProxy,
      "/research": apiProxy,
    },
  },
});

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],

  server: {
    host: "0.0.0.0",
    port: 1420,
    strictPort: true,

    proxy: {
      "/video": {
        target:
          "http://127.0.0.1:8787",
        changeOrigin: true,
      },

      "/subtitles": {
        target:
          "http://127.0.0.1:8787",
        changeOrigin: true,

        rewrite: (path) =>
          path.replace(
            /^\/subtitles/,
            "/media",
          ),
      },

      "/api": {
        target:
          "http://127.0.0.1:8787",
        changeOrigin: true,
      },
    },
  },
});
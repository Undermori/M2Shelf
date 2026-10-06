import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host ?? "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Packaging creates/removes executable files which Windows may temporarily lock.
      // These generated files never contribute to frontend hot reload.
      ignored: ["**/src-tauri/**", "**/bundle/**", "**/.tmp/**", "**/.tools/**", "**/output/playwright/**"],
    },
  },
});

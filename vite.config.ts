import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Two pages: the settings window and the recording overlay.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "safari15",
    rollupOptions: {
      input: {
        main: "index.html",
        overlay: "overlay.html",
      },
    },
  },
});

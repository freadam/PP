import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";

// Fruit is a desktop app: the dev server exists only for the Tauri window and
// for looking at the UI in a browser. There is no deploy target.
export default defineConfig({
  plugins: [react(), tailwind()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    // Devtools are disabled in release (§7.3); source maps stay off so the
    // shipped bundle carries no extra surface.
    sourcemap: false,
    outDir: "dist",
  },
});

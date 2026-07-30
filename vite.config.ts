import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";

// Fruit is a desktop app: the dev server exists only for the Tauri window and
// for looking at the UI in a browser. There is no deploy target.
export default defineConfig({
  plugins: [react(), tailwind()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Never watch the Rust side. `cargo` writes proc-macro DLLs into
      // src-tauri/target while it compiles, and on Windows the watcher grabs
      // one mid-write and dies with EBUSY — taking the whole dev command with
      // it. Nothing under here is a frontend source file anyway.
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },
  build: {
    target: "es2022",
    // Devtools are disabled in release (§7.3); source maps stay off so the
    // shipped bundle carries no extra surface.
    sourcemap: false,
    outDir: "dist",
  },
});

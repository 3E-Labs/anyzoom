import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // relative paths: the built files are served from inside the app, not a web server
  base: "./",
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
});

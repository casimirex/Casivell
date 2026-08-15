import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `base: "./"` keeps the build relocatable: the page and the `.wasm` it fetches
// resolve relative to wherever the bundle is served, so the same `dist/` works at
// a domain root or under a sub-path without a rebuild.
export default defineConfig({
  plugins: [react()],
  base: "./",
});

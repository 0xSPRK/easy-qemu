import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [
    svelte({
      onwarn(w, defaultHandler) {
        // VmForm intentionally captures `existing` once at mount (the modal
        // is re-created for every open), so this warning is noise here.
        if (w.code === "state_referenced_locally") return;
        defaultHandler?.(w);
      },
    }),
  ],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "es2022",
  },
});


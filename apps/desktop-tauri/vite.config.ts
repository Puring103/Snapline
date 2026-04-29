import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) {
            return undefined;
          }
          if (id.includes("@tiptap") || id.includes("prosemirror")) {
            return "vendor-tiptap";
          }
          if (id.includes("lowlight") || id.includes("highlight.js")) {
            return "vendor-highlight";
          }
          if (id.includes("react") || id.includes("react-dom")) {
            return "vendor-react";
          }
          return undefined;
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  test: {
    environment: "jsdom",
  },
});

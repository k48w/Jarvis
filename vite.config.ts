import { defineConfig } from "vite";

export default defineConfig({
  server: { watch: { ignored: ["**/src-tauri/**", "**/docs/**"] } },
  build: {
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-react": ["react", "react-dom"],
          "vendor-three": ["three", "@react-three/fiber"],
          "vendor-icons": ["lucide-react"],
        },
      },
    },
  },
});

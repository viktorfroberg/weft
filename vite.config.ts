import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// React Compiler — stable in React 19. Auto-memoizes components so most
// useMemo/useCallback calls become noise you don't have to write. Knock-
// on effect: Zustand selector ref-stability bugs get harder to introduce
// since the compiler won't re-create arrays/objects across renders when
// their inputs are unchanged. Target "19" matches our React major.
const reactCompilerConfig = { target: "19" } as const;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    react({
      babel: {
        plugins: [["babel-plugin-react-compiler", reactCompilerConfig]],
      },
    }),
    tailwindcss(),
  ],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Keep Monaco + xterm out of the main entry chunk. Both are heavy:
  // Monaco is ~300 KB gzip on its own, xterm + addons ~40 KB. Putting
  // them in their own chunks lets `React.lazy(DiffViewer)` + the
  // Terminal mount defer the cost until a user actually opens a diff
  // or a terminal tab, and keeps the main app shell responsive on
  // cold start.
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          monaco: ["@monaco-editor/react", "monaco-editor"],
          xterm: [
            "@xterm/xterm",
            "@xterm/addon-fit",
            "@xterm/addon-web-links",
            "@xterm/addon-search",
          ],
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));

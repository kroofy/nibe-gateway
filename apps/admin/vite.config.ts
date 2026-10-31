import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";

// The release version this UI ships with (workspace Cargo.toml), to notice a newer gateway.
const releaseVersion =
  /\[workspace\.package\][^[]*?version\s*=\s*"([^"]+)"/s.exec(
    readFileSync(new URL("../../Cargo.toml", import.meta.url), "utf8"),
  )?.[1] ?? "unknown";

export default defineConfig({
  plugins: [react()],
  define: { __UI_VERSION__: JSON.stringify(releaseVersion) },
  server: {
    proxy: Object.fromEntries(
      ["/api", "/openapi.json", "/health", "/__dev"].map((path) => [
        path,
        {
          target: process.env.GATEWAY_URL || "http://127.0.0.1:8080",
          changeOrigin: false,
        },
      ]),
    ),
  },
  build: { target: "es2020", sourcemap: false },
});

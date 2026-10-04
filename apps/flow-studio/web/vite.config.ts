import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Both ports are set in ../config.json and neither is a default; the dev
// server forwards the wire to the studio's server, so the page and the wire
// share one origin.
const config = JSON.parse(
  readFileSync(
    fileURLToPath(new URL("../config.json", import.meta.url)),
    "utf8",
  ),
) as { server_port: number; web_port: number };
const server = `http://127.0.0.1:${config.server_port}`;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    host: "127.0.0.1",
    port: config.web_port,
    strictPort: true,
    proxy: { "/v1": server, "/sandbox": server, "/studio": server },
  },
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
  },
});

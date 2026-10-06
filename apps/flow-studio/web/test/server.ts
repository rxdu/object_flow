// The studio's real server on a free loopback port, for a test file, its wire
// answered by the backend named. It needs the server's virtual environment
// (apps/flow-studio/README.md), and, for the core, the pinned Rust toolchain;
// it fails, not skips, without them.

import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { type Backend, type Wire, httpWire } from "../src/api/wire";

const SERVER = fileURLToPath(new URL("../../server", import.meta.url));

/** The backends every wire test runs against. */
export const BACKENDS: Backend["backend"][] = ["runner", "core"];

/** A server whose workspace is a fresh directory of its own, so no test
 * writes the author's. */
export async function startServer(
  backend: Backend["backend"],
): Promise<{ wire: Wire; stop: () => void }> {
  const workspace = mkdtempSync(join(tmpdir(), "flow-studio-"));
  const server = spawn(
    ".venv/bin/python",
    [
      "-m",
      "flowstudio",
      "--port",
      "0",
      "--workspace",
      workspace,
      "--backend",
      backend,
    ],
    {
      cwd: SERVER,
    },
  );
  const base = await new Promise<string>((resolve, reject) => {
    let log = "";
    server.stderr.on("data", (chunk: Buffer) => {
      log += chunk.toString();
      const m = /listening on (http:\/\/127\.0\.0\.1:\d+)/.exec(log);
      if (m?.[1]) resolve(m[1]);
    });
    server.on("exit", (code) =>
      reject(new Error(`the server exited (${code}): ${log}`)),
    );
    server.on("error", reject);
  });
  const wire = httpWire(base);
  // the wire answered by the backend asked for, so a test named for the core
  // never passes on the runner
  const answering = (await wire.backend()).backend;
  if (answering !== backend) {
    server.kill();
    throw new Error(`asked for the ${backend}, answered by the ${answering}`);
  }
  return { wire, stop: () => server.kill() };
}

// The studio's real server on a free loopback port, for a test file. It
// needs the server's virtual environment (apps/flow-studio/README.md), and
// fails, not skips, without it.

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { type Wire, httpWire } from "../src/api/wire";

const SERVER = fileURLToPath(new URL("../../server", import.meta.url));

export async function startServer(): Promise<{ wire: Wire; stop: () => void }> {
  const server = spawn(
    ".venv/bin/python",
    ["-m", "flowstudio", "--port", "0"],
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
  return { wire: httpWire(base), stop: () => server.kill() };
}

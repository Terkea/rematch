import { spawn } from "node:child_process";

/** Starts Stockfish, reads its UCI banner line (e.g. "Stockfish 19 by ...") and quits. */
export function getStockfishVersion(path: string, timeoutMs = 5000): Promise<string> {
  return new Promise((resolve, reject) => {
    const proc = spawn(path, [], { stdio: ["pipe", "pipe", "inherit"] });
    let out = "";
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      proc.kill();
      fn();
    };
    const timer = setTimeout(
      () => finish(() => reject(new Error(`stockfish did not respond within ${timeoutMs} ms`))),
      timeoutMs,
    );
    proc.on("error", (err) => finish(() => reject(err)));
    proc.stdout.on("data", (chunk: Buffer) => {
      out += chunk.toString();
      const line = out.split("\n")[0];
      if (line?.startsWith("Stockfish")) finish(() => resolve(line.trim()));
    });
    proc.on("close", () =>
      finish(() => reject(new Error("stockfish exited before printing its version"))),
    );
    proc.stdin.write("uci\n");
  });
}

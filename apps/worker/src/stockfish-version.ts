import { spawn } from "node:child_process";

/** Starts Stockfish, reads its UCI banner line (e.g. "Stockfish 19 by ...") and quits. */
export function getStockfishVersion(path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const proc = spawn(path, [], { stdio: ["pipe", "pipe", "inherit"] });
    let out = "";
    proc.on("error", reject);
    proc.stdout.on("data", (chunk: Buffer) => {
      out += chunk.toString();
      const line = out.split("\n")[0];
      if (line?.startsWith("Stockfish")) {
        proc.kill();
        resolve(line.trim());
      }
    });
    proc.on("close", () => reject(new Error("stockfish exited before printing its version")));
    proc.stdin.write("uci\n");
  });
}

import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { pathToFileURL } from "node:url";
import { handleChesscom } from "./chesscom.js";
import { handleLichess, lastTokenRequest } from "./lichess.js";

// The provider fakes on one port: Lichess OAuth (`/oauth`, `/api/token`, `/api/account`,
// `/api/account/email`) and chess.com (`/chesscom/...`). Unit tests start them on a random port
// with `startFakes(0)`; the compose `fakes` service (profile e2e) runs this file on port 3199.

export type Fakes = {
  url: string;
  close(): Promise<void>;
  lastTokenRequest(): URLSearchParams | null;
};

export function startFakes(port: number, host = "127.0.0.1"): Promise<Fakes> {
  const server = createServer((req, res) => {
    handleLichess(req, res)
      .then((handled) => handled || handleChesscom(req, res))
      .then((handled) => {
        if (handled) return;
        res.writeHead(404, { "content-type": "application/json" });
        res.end(JSON.stringify({ message: "not found" }));
      })
      .catch((error: unknown) => {
        console.error(error);
        res.writeHead(500);
        res.end();
      });
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, host, () => {
      const { port: bound } = server.address() as AddressInfo;
      const shown = host === "0.0.0.0" ? "127.0.0.1" : host;
      resolve({
        url: `http://${shown}:${bound}`,
        lastTokenRequest,
        close: () =>
          new Promise<void>((done, fail) => {
            server.closeAllConnections();
            server.close((error) => (error ? fail(error) : done()));
          }),
      });
    });
  });
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const port = Number(process.env.PORT ?? 3199);
  startFakes(port, "0.0.0.0").then(
    () => console.log(`fakes listening on ${port}`),
    (error: unknown) => {
      console.error(error);
      process.exit(1);
    },
  );
}

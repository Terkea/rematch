import { createServer } from "node:http";
import { handleChesscom } from "./chesscom.js";

// The compose `fakes` service (profile e2e): the provider fakes on one port. Task 21 serves the
// chess.com fake; Task 18 adds the Lichess OAuth fake (`/oauth`, `/api/token`, `/api/account`).

const port = Number(process.env.PORT ?? 3199);

const server = createServer((req, res) => {
  handleChesscom(req, res)
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

server.listen(port, "0.0.0.0", () => {
  console.log(`fakes listening on ${port}`);
});

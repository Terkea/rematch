import { getStockfishVersion } from "./stockfish-version.js";

const stockfishPath = process.env.STOCKFISH_PATH;
if (!stockfishPath) throw new Error("STOCKFISH_PATH is not set");

console.log("worker ready");
console.log(await getStockfishVersion(stockfishPath));

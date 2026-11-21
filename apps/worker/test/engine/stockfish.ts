// Shared helper: engine tests need the real binary. They fail (not skip) in Docker.
export const stockfishPath = process.env.STOCKFISH_PATH ?? "";
export const inDocker = process.env.IN_DOCKER === "1";
export const skipEngineTests = !inDocker && !stockfishPath;

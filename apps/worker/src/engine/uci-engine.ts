import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { Chess, validateFen } from "chess.js";

// TODO(T11): import type { EngineScore } from "@ci/chess-core"
/** Structurally identical to `EngineScore` from `@ci/chess-core`. */
export type EngineScore = { cp: number } | { mate: number };

export interface AnalysisLine {
  multipv: number;
  /** From the side-to-move's point of view. */
  score: EngineScore;
  pv: string[];
  depth: number;
  nodes: number;
}

export class InvalidFenError extends Error {
  constructor(fen: string, reason?: string) {
    super(`Invalid FEN: ${JSON.stringify(fen)}${reason ? ` (${reason})` : ""}`);
    this.name = "InvalidFenError";
  }
}

/** The engine process exited while a command was in flight (retryable). */
export class EngineCrashedError extends Error {
  constructor(message = "Engine process exited unexpectedly") {
    super(message);
    this.name = "EngineCrashedError";
  }
}

export class EngineTimeoutError extends Error {
  constructor(ms: number) {
    super(`Engine analysis timed out after ${ms} ms`);
    this.name = "EngineTimeoutError";
  }
}

export const DEFAULT_ANALYSIS_TIMEOUT_MS = 30_000;
const HANDSHAKE_TIMEOUT_MS = 10_000;

export interface StartOptions {
  path: string;
  threads?: number;
  hashMb?: number;
  syzygyPath?: string;
}

export interface AnalyseOptions {
  nodes: number;
  multipv?: number;
  timeoutMs?: number;
}

/** Parse one `info` line; returns null for lines that must be ignored. */
export function parseInfoLine(line: string): AnalysisLine | null {
  if (!line.startsWith("info ")) return null;
  const t = line.split(/\s+/);
  if (t.includes("lowerbound") || t.includes("upperbound")) return null;
  const pvIdx = t.indexOf("pv");
  const scoreIdx = t.indexOf("score");
  if (pvIdx < 0 || scoreIdx < 0 || pvIdx === t.length - 1) return null;
  const kind = t[scoreIdx + 1];
  const value = Number(t[scoreIdx + 2]);
  if (!Number.isFinite(value)) return null;
  let score: EngineScore;
  if (kind === "cp") score = { cp: value };
  else if (kind === "mate") score = { mate: value };
  else return null;
  const num = (key: string): number => {
    const i = t.indexOf(key);
    const n = i >= 0 ? Number(t[i + 1]) : 0;
    return Number.isFinite(n) ? n : 0;
  };
  return {
    multipv: t.includes("multipv") ? num("multipv") : 1,
    score,
    pv: t.slice(pvIdx + 1),
    depth: num("depth"),
    nodes: num("nodes"),
  };
}

/** Stockfish answers `bestmove (none)` with no pv for terminal positions. */
function terminalLines(fen: string): AnalysisLine[] {
  try {
    const chess = new Chess(fen);
    if (chess.isCheckmate())
      return [{ multipv: 1, score: { mate: 0 }, pv: [], depth: 0, nodes: 0 }];
    if (chess.isStalemate()) return [{ multipv: 1, score: { cp: 0 }, pv: [], depth: 0, nodes: 0 }];
  } catch {
    // fall through
  }
  return [];
}

type LineListener = (line: string) => void;

export class UciEngine {
  private alive_ = true;
  private listener: LineListener | null = null;
  private rejectPending: ((e: Error) => void) | null = null;
  private chain: Promise<unknown> = Promise.resolve();

  private constructor(private readonly child: ChildProcessWithoutNullStreams) {
    const rl = createInterface({ input: child.stdout });
    rl.on("line", (l) => this.listener?.(l.trim()));
    child.stdin.on("error", () => {});
    child.stderr.on("data", () => {});
    const dead = () => {
      this.alive_ = false;
      const r = this.rejectPending;
      this.rejectPending = null;
      r?.(new EngineCrashedError());
    };
    child.on("exit", dead);
    child.on("close", dead);
    child.on("error", dead);
  }

  get alive(): boolean {
    return this.alive_;
  }

  get pid(): number | undefined {
    return this.child.pid;
  }

  static async start(opts: StartOptions): Promise<UciEngine> {
    const child = spawn(opts.path, [], { stdio: ["pipe", "pipe", "pipe"] });
    const engine = new UciEngine(child);
    try {
      engine.send("uci");
      await engine.waitFor("uciok", HANDSHAKE_TIMEOUT_MS);
      if (opts.threads !== undefined) engine.send(`setoption name Threads value ${opts.threads}`);
      if (opts.hashMb !== undefined) engine.send(`setoption name Hash value ${opts.hashMb}`);
      if (opts.syzygyPath) engine.send(`setoption name SyzygyPath value ${opts.syzygyPath}`);
      engine.send("isready");
      await engine.waitFor("readyok", HANDSHAKE_TIMEOUT_MS);
    } catch (e) {
      engine.kill();
      throw e;
    }
    return engine;
  }

  analyse(fen: string, opts: AnalyseOptions): Promise<AnalysisLine[]> {
    // Validate before queueing; invalid input must never reach the engine.
    const v = validateFen(fen);
    if (!v.ok) return Promise.reject(new InvalidFenError(fen, v.error));
    const run = () => this.runAnalysis(fen, opts);
    const p = this.chain.then(run, run);
    this.chain = p.catch(() => {});
    return p;
  }

  private async runAnalysis(fen: string, opts: AnalyseOptions): Promise<AnalysisLine[]> {
    if (!this.alive_) throw new EngineCrashedError("Engine is not running");
    const nodes = Math.max(1, Math.floor(opts.nodes));
    const multipv = Math.max(1, Math.floor(opts.multipv ?? 1));
    const timeoutMs = opts.timeoutMs ?? DEFAULT_ANALYSIS_TIMEOUT_MS;
    this.send(`setoption name MultiPV value ${multipv}`);
    this.send("ucinewgame");
    this.send("isready");
    await this.waitFor("readyok", timeoutMs);
    this.send(`position fen ${fen}`);
    const best = new Map<number, AnalysisLine>();
    this.send(`go nodes ${nodes}`);
    await this.waitFor("bestmove", timeoutMs, (line) => {
      const info = parseInfoLine(line);
      if (info) best.set(info.multipv, info);
    });
    if (best.size === 0) return terminalLines(fen);
    return [...best.values()].sort((a, b) => a.multipv - b.multipv);
  }

  async quit(): Promise<void> {
    if (!this.alive_) return;
    const closed = new Promise<void>((resolve) => this.child.once("close", () => resolve()));
    this.send("quit");
    const timer = setTimeout(() => this.kill(), 1000);
    await closed;
    clearTimeout(timer);
  }

  private kill(): void {
    this.alive_ = false;
    this.child.kill("SIGKILL");
  }

  private send(cmd: string): void {
    if (this.alive_ && this.child.stdin.writable) this.child.stdin.write(`${cmd}\n`);
  }

  /** Resolves when a line starting with `prefix` arrives; rejects on death or timeout. */
  private waitFor(prefix: string, timeoutMs: number, onLine?: LineListener): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      if (!this.alive_) return reject(new EngineCrashedError());
      const finish = () => {
        clearTimeout(timer);
        this.listener = null;
        this.rejectPending = null;
      };
      const timer = setTimeout(() => {
        finish();
        this.kill();
        reject(new EngineTimeoutError(timeoutMs));
      }, timeoutMs);
      this.rejectPending = (e) => {
        finish();
        reject(e);
      };
      this.listener = (line) => {
        if (line === prefix || line.startsWith(`${prefix} `)) {
          finish();
          resolve();
        } else onLine?.(line);
      };
    });
  }
}

import { validateFen } from "chess.js";
import {
  type AnalyseOptions,
  type AnalysisLine,
  EngineCrashedError,
  InvalidFenError,
  UciEngine,
} from "./uci-engine.js";

export class EnginePoolClosedError extends Error {
  constructor() {
    super("Engine pool is closed");
    this.name = "EnginePoolClosedError";
  }
}

export interface PoolOptions {
  size: number;
  path: string;
  hashMb?: number;
}

export class EnginePool {
  private idle: UciEngine[] = [];
  private waiters: Array<{ resolve: (e: UciEngine) => void; reject: (e: Error) => void }> = [];
  private spawning = new Set<Promise<UciEngine>>();
  private all = new Set<UciEngine>();
  private closed = false;

  private constructor(private readonly opts: PoolOptions) {}

  static async create(opts: PoolOptions): Promise<EnginePool> {
    const pool = new EnginePool(opts);
    try {
      for (let i = 0; i < opts.size; i++) {
        const e = await pool.spawn();
        pool.idle.push(e);
      }
    } catch (err) {
      await pool.close();
      throw err;
    }
    return pool;
  }

  /** Process ids of live engines (useful for supervision tests). */
  pids(): number[] {
    return [...this.all].flatMap((e) => (e.alive && e.pid !== undefined ? [e.pid] : []));
  }

  async analyse(fen: string, opts: AnalyseOptions): Promise<AnalysisLine[]> {
    if (this.closed) throw new EnginePoolClosedError();
    const v = validateFen(fen);
    if (!v.ok) throw new InvalidFenError(fen, v.error);
    let engine = await this.acquire();
    try {
      for (let attempt = 0; ; attempt++) {
        try {
          if (!engine.alive) engine = await this.replace(engine);
          const lines = await engine.analyse(fen, opts);
          // close() interrupts the search; a truncated result must not be returned.
          if (this.closed) throw new EnginePoolClosedError();
          return lines;
        } catch (err) {
          // A dead process is restarted and the job retried once.
          // Timeouts and invalid FENs are surfaced as-is.
          if (this.closed) throw new EnginePoolClosedError();
          if (err instanceof EngineCrashedError && attempt === 0) {
            engine = await this.replace(engine);
            continue;
          }
          throw err;
        }
      }
    } finally {
      if (!engine.alive && !this.closed) {
        // Restart so the slot is not lost (e.g. after a timeout kill).
        engine = await this.replace(engine).catch(() => engine);
      }
      this.release(engine);
    }
  }

  /**
   * Rejects queued jobs and in-flight jobs (their engines are quit) with
   * EnginePoolClosedError; resolves once every engine has exited.
   */
  async close(): Promise<void> {
    this.closed = true;
    for (const w of this.waiters.splice(0)) w.reject(new EnginePoolClosedError());
    this.idle = [];
    await Promise.allSettled([...this.spawning]);
    const engines = [...this.all];
    this.all.clear();
    await Promise.all(engines.map((e) => e.quit().catch(() => {})));
  }

  private spawn(): Promise<UciEngine> {
    if (this.closed) return Promise.reject(new EnginePoolClosedError());
    const p = UciEngine.start({ path: this.opts.path, hashMb: this.opts.hashMb }).then(
      async (e) => {
        if (this.closed) {
          await e.quit().catch(() => {});
          throw new EnginePoolClosedError();
        }
        this.all.add(e);
        return e;
      },
    );
    this.spawning.add(p);
    const done = () => this.spawning.delete(p);
    p.then(done, done);
    return p;
  }

  private async replace(dead: UciEngine): Promise<UciEngine> {
    this.all.delete(dead);
    await dead.quit().catch(() => {});
    return this.spawn();
  }

  private acquire(): Promise<UciEngine> {
    const e = this.idle.pop();
    if (e) return Promise.resolve(e);
    return new Promise((resolve, reject) => this.waiters.push({ resolve, reject }));
  }

  private release(e: UciEngine): void {
    if (this.closed) return;
    const w = this.waiters.shift();
    if (w) w.resolve(e);
    else this.idle.push(e);
  }
}

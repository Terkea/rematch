import {
  type AnalyseOptions,
  type AnalysisLine,
  EngineCrashedError,
  UciEngine,
} from "./uci-engine.js";

export interface PoolOptions {
  size: number;
  path: string;
  hashMb?: number;
}

export class EnginePool {
  private idle: UciEngine[] = [];
  private waiters: Array<(e: UciEngine) => void> = [];
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
    if (this.closed) throw new Error("Engine pool is closed");
    let engine = await this.acquire();
    try {
      for (let attempt = 0; ; attempt++) {
        try {
          if (!engine.alive) engine = await this.replace(engine);
          return await engine.analyse(fen, opts);
        } catch (err) {
          // A dead process is restarted and the job retried once.
          // Timeouts and invalid FENs are surfaced as-is.
          if (err instanceof EngineCrashedError && attempt === 0 && !this.closed) {
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

  async close(): Promise<void> {
    this.closed = true;
    const engines = [...this.all];
    this.all.clear();
    this.idle = [];
    await Promise.all(engines.map((e) => e.quit().catch(() => {})));
  }

  private spawn(): Promise<UciEngine> {
    return UciEngine.start({ path: this.opts.path, hashMb: this.opts.hashMb }).then((e) => {
      this.all.add(e);
      return e;
    });
  }

  private async replace(dead: UciEngine): Promise<UciEngine> {
    this.all.delete(dead);
    await dead.quit().catch(() => {});
    return this.spawn();
  }

  private acquire(): Promise<UciEngine> {
    const e = this.idle.pop();
    if (e) return Promise.resolve(e);
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  private release(e: UciEngine): void {
    const w = this.waiters.shift();
    if (w) w(e);
    else this.idle.push(e);
  }
}

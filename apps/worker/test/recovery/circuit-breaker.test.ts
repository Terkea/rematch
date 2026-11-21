import { describe, expect, it } from "vitest";
import {
  CircuitOpenError,
  createCircuitBreaker,
  withCircuitBreaker,
} from "../../src/http/circuit-breaker.js";
import { PlatformNotFoundError } from "../../src/http/platform-client.js";

const fail = () => Promise.reject(new Error("HTTP 503"));
const ok = () => Promise.resolve("ok");

describe("circuit breaker", () => {
  it("circuit opens after 5 failures", async () => {
    let t = 1_000;
    const breaker = createCircuitBreaker({
      failureThreshold: 5,
      cooldownMs: 60_000,
      now: () => t,
    });
    for (let i = 0; i < 5; i++) await expect(breaker.exec(fail)).rejects.toThrow("HTTP 503");
    expect(breaker.state()).toBe("open");

    let called = false;
    await expect(
      breaker.exec(async () => {
        called = true;
        return 1;
      }),
    ).rejects.toBeInstanceOf(CircuitOpenError);
    expect(called).toBe(false);

    t += 59_999;
    expect(breaker.state()).toBe("open");
    t += 1;
    expect(breaker.state()).toBe("half-open");
    await expect(breaker.exec(ok)).resolves.toBe("ok");
    expect(breaker.state()).toBe("closed");
  });

  it("uses the brief's defaults: 5 failures, 60 s cooldown", async () => {
    let t = 0;
    const breaker = createCircuitBreaker({ now: () => t });
    for (let i = 0; i < 4; i++) await breaker.exec(fail).catch(() => {});
    expect(breaker.state()).toBe("closed");
    await breaker.exec(fail).catch(() => {});
    expect(breaker.state()).toBe("open");
    t = 60_000;
    expect(breaker.state()).toBe("half-open");
  });

  it("a failed probe re-opens for another cooldown", async () => {
    let t = 0;
    const breaker = createCircuitBreaker({ failureThreshold: 2, cooldownMs: 100, now: () => t });
    await breaker.exec(fail).catch(() => {});
    await breaker.exec(fail).catch(() => {});
    t = 100;
    await expect(breaker.exec(fail)).rejects.toThrow("HTTP 503");
    expect(breaker.state()).toBe("open");
    t = 199;
    expect(breaker.state()).toBe("open");
    t = 200;
    expect(breaker.state()).toBe("half-open");
  });

  it("allows only one probe at a time while half-open", async () => {
    let t = 0;
    const breaker = createCircuitBreaker({ failureThreshold: 1, cooldownMs: 10, now: () => t });
    await breaker.exec(fail).catch(() => {});
    t = 10;
    let release!: () => void;
    const probe = breaker.exec(() => new Promise<void>((r) => (release = r)));
    await expect(breaker.exec(ok)).rejects.toBeInstanceOf(CircuitOpenError);
    release();
    await probe;
    expect(breaker.state()).toBe("closed");
  });

  it("counts consecutive failures only", async () => {
    const breaker = createCircuitBreaker({ failureThreshold: 3, cooldownMs: 10 });
    await breaker.exec(fail).catch(() => {});
    await breaker.exec(fail).catch(() => {});
    await breaker.exec(ok);
    await breaker.exec(fail).catch(() => {});
    await breaker.exec(fail).catch(() => {});
    expect(breaker.state()).toBe("closed");
  });

  it("ignores errors that are not outages (404)", async () => {
    const breaker = createCircuitBreaker({
      failureThreshold: 1,
      cooldownMs: 10,
      isFailure: (err) => !(err instanceof PlatformNotFoundError),
    });
    await expect(
      breaker.exec(() => Promise.reject(new PlatformNotFoundError("u"))),
    ).rejects.toThrow(PlatformNotFoundError);
    expect(breaker.state()).toBe("closed");
  });

  it("wraps a platform client's getJson and getNdjson", async () => {
    const breaker = createCircuitBreaker({ failureThreshold: 1, cooldownMs: 60_000 });
    let calls = 0;
    const client = withCircuitBreaker(
      {
        getJson: async <T>() => {
          calls++;
          throw new Error("down") as unknown as T;
        },
        getNdjson: async function* <T>() {
          calls++;
          yield { n: 1 } as T;
        },
      },
      breaker,
    );
    await expect(client.getJson("u")).rejects.toThrow("down");
    expect(breaker.state()).toBe("open");
    const iterate = async () => {
      for await (const _ of client.getNdjson("u")) {
        // drain
      }
    };
    await expect(iterate()).rejects.toBeInstanceOf(CircuitOpenError);
    expect(calls).toBe(1);
  });
});

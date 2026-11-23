import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  fromGitHub,
  fromSeed,
  loadSeed,
  milestoneSearchUrl,
  renderReadmeBlock,
  replaceBlock,
} from "../../scripts/roadmap/roadmap-lib.mjs";
import { loadRoadmap } from "../scripts/roadmap-data.mjs";

const ROOT = resolve(import.meta.dirname, "..", "..");
const seed = loadSeed(ROOT);

const ok = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });

describe("the roadmap model", () => {
  it("from the seed: every milestone, its epics and their sub-issue counts, no fake progress", () => {
    const model = fromSeed(seed);
    expect(model.milestones.map((m: { title: string }) => m.title)).toEqual(
      seed.milestones.map((m: { title: string }) => m.title),
    );
    const first = model.milestones[0];
    expect(first?.closed).toBeNull();
    expect(first?.epics.length).toBeGreaterThan(0);
    for (const e of first?.epics ?? []) {
      expect(e.done).toBeNull();
      expect(e.total).toBeGreaterThan(0);
    }
  });

  it("from GitHub: open milestones in seed order, epics with done of total, horizon from the seed", () => {
    const [a, b] = seed.milestones;
    const api = [
      {
        number: 2,
        title: b?.title,
        state: "open",
        open_issues: 3,
        closed_issues: 1,
        html_url: "u2",
      },
      {
        number: 1,
        title: a?.title,
        state: "open",
        open_issues: 4,
        closed_issues: 2,
        html_url: "u1",
      },
      { number: 7, title: "v0.1 Shipped", state: "closed", open_issues: 0, closed_issues: 9 },
      { number: 8, title: "Someday", state: "open", open_issues: 1, closed_issues: 0 },
    ];
    const epics = [
      {
        number: 5,
        title: "Epic: x",
        state: "open",
        html_url: "e5",
        milestone: { number: 1 },
        sub_issues_summary: { completed: 1, total: 3 },
      },
      { number: 6, title: "A PR", pull_request: {}, milestone: { number: 1 } },
    ];
    const model = fromGitHub(seed, api, epics);
    expect(model.milestones.map((m: { title: string }) => m.title)).toEqual([
      a?.title,
      b?.title,
      "Someday",
    ]);
    expect(model.milestones[0]).toMatchObject({ closed: 2, total: 6, horizon: a?.horizon });
    expect(model.milestones[0]?.epics).toEqual([
      { title: "Epic: x", url: "e5", state: "open", done: 1, total: 3 },
    ]);
    expect(model.milestones[2]?.horizon).toBe("later");
  });

  it("links a milestone by search before it has a number", () => {
    expect(milestoneSearchUrl("v0.2 Practice habit")).toBe(
      "https://github.com/Terkea/rematch/issues?q=is%3Aissue+milestone%3A%22v0.2+Practice+habit%22",
    );
  });
});

describe("loadRoadmap", () => {
  it("reads GitHub when it has the roadmap's milestones", async () => {
    const title = seed.milestones[0]?.title;
    const fetchImpl = async (url: string | URL | Request) =>
      String(url).includes("/milestones")
        ? ok([{ number: 1, title, state: "open", open_issues: 1, closed_issues: 1 }])
        : ok([]);
    const r = await loadRoadmap(seed, { live: true, fetchImpl: fetchImpl as typeof fetch });
    expect(r.model.source).toBe("github");
  });

  it("falls back to the seed when GitHub refuses, is unreachable or has no roadmap yet", async () => {
    const refuses = async () => new Response("", { status: 404 });
    const throws = async () => {
      throw new Error("offline");
    };
    const empty = async () => ok([]);
    for (const f of [refuses, throws, empty]) {
      const r = await loadRoadmap(seed, { live: true, fetchImpl: f as unknown as typeof fetch });
      expect(r.model.source).toBe("seed");
      expect(r.note).toMatch(/^seed files/);
    }
    expect((await loadRoadmap(seed)).model.source).toBe("seed");
  });
});

describe("the README block", () => {
  it("replaces only what is between the markers, and is stable", () => {
    const block = renderReadmeBlock(seed);
    const text = `# R\n\nbefore\n\n<!-- roadmap:start -->\nold\n<!-- roadmap:end -->\n\nafter\n`;
    const once = replaceBlock(text, block);
    expect(once).toContain("before");
    expect(once).toContain("after");
    expect(once).not.toContain("old");
    expect(replaceBlock(once, block)).toBe(once);
    expect(() => replaceBlock("no markers", block)).toThrow(/markers/);
  });
});

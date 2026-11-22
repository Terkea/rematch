import { describe, expect, it } from "vitest";
import {
  type GameListQuery,
  parseGameQuery,
  playerResult,
  serializeGameQuery,
} from "../../src/server/games";

const DEFAULTS: GameListQuery = {
  filters: { timeClass: null, result: null, colour: null, from: null, to: null, accountId: null },
  sort: { key: "played", dir: "desc" },
  page: 1,
};

describe("parseGameQuery", () => {
  it("gives the defaults for no params", () => {
    expect(parseGameQuery({})).toEqual(DEFAULTS);
  });

  it("falls back on bad input", () => {
    expect(
      parseGameQuery({
        page: "-3",
        tc: "hyper",
        from: "2026-13-40",
        to: "x",
        sort: "drop",
        colour: "red",
        result: ["win", "loss"],
      }),
    ).toEqual(DEFAULTS);
    expect(parseGameQuery({ account: "not-a-uuid" }).filters.accountId).toBeNull();
    expect(parseGameQuery({ page: "2.5" }).page).toBe(1);
    expect(parseGameQuery({ page: "0" }).page).toBe(1);
    expect(parseGameQuery({ from: "2026-02-30" }).filters.from).toBeNull();
    expect(parseGameQuery({ from: "0100-01-01" }).filters.from).toBeNull();
    expect(parseGameQuery({ to: "2101-01-01" }).filters.to).toBeNull();
  });

  it("ignores prototype keys in every enum param", () => {
    for (const key of ["constructor", "__proto__", "toString", "hasOwnProperty", "valueOf"]) {
      const q = parseGameQuery({ sort: key, tc: key, result: key, colour: key });
      expect(q).toEqual(DEFAULTS);
      expect(serializeGameQuery(q).toString()).toBe("");
    }
  });

  it("parses every param", () => {
    expect(
      parseGameQuery({
        tc: "rapid",
        result: "loss",
        colour: "black",
        from: "2026-10-01",
        to: "2026-10-31",
        sort: "accuracy-asc",
        page: "3",
      }),
    ).toEqual({
      filters: {
        timeClass: "rapid",
        result: "loss",
        colour: "black",
        from: "2026-10-01",
        to: "2026-10-31",
        accountId: null,
      },
      sort: { key: "accuracy", dir: "asc" },
      page: 3,
    });
    const id = "0b9e6f8c-3c1a-4d8e-9f00-6a2b1c3d4e5f";
    expect(parseGameQuery({ account: id }).filters.accountId).toBe(id);
  });

  it("swaps from and to when from is later", () => {
    const q = parseGameQuery({ from: "2026-11-05", to: "2026-10-01" });
    expect(q.filters.from).toBe("2026-10-01");
    expect(q.filters.to).toBe("2026-11-05");
  });
});

describe("serializeGameQuery", () => {
  it("omits defaults in a fixed key order", () => {
    expect(
      serializeGameQuery(parseGameQuery({ page: "3", result: "loss", tc: "rapid" })).toString(),
    ).toBe("tc=rapid&result=loss&page=3");
    expect(serializeGameQuery(parseGameQuery({})).toString()).toBe("");
  });

  it("round-trips every param", () => {
    const params = {
      tc: "daily",
      result: "draw",
      colour: "white",
      from: "2026-01-01",
      to: "2026-02-01",
      account: "0b9e6f8c-3c1a-4d8e-9f00-6a2b1c3d4e5f",
      sort: "blunders-desc",
      page: "4",
    };
    const once = serializeGameQuery(parseGameQuery(params));
    expect(once.toString()).toBe(
      "tc=daily&result=draw&colour=white&from=2026-01-01&to=2026-02-01&account=0b9e6f8c-3c1a-4d8e-9f00-6a2b1c3d4e5f&sort=blunders-desc&page=4",
    );
    const again = serializeGameQuery(parseGameQuery(Object.fromEntries(once)));
    expect(again.toString()).toBe(once.toString());
  });
});

describe("serializeGameQuery validation", () => {
  it("writes only valid values", () => {
    const bad = {
      filters: {
        timeClass: "hyper",
        result: "maybe",
        colour: "red",
        from: "2026-13-40",
        to: "soon",
        accountId: "not-a-uuid",
      },
      sort: { key: "constructor", dir: "sideways" },
      page: 2.5,
    } as unknown as GameListQuery;
    expect(serializeGameQuery(bad).toString()).toBe("");
    expect(serializeGameQuery({ ...DEFAULTS, page: 0 }).toString()).toBe("");
    expect(serializeGameQuery({ ...DEFAULTS, page: Number.NaN }).toString()).toBe("");
  });

  it("round-trips every sort", () => {
    for (const sort of [
      "played-asc",
      "accuracy-desc",
      "accuracy-asc",
      "opponent-desc",
      "blunders-desc",
    ]) {
      expect(serializeGameQuery(parseGameQuery({ sort })).toString()).toBe(`sort=${sort}`);
    }
    expect(serializeGameQuery(parseGameQuery({ sort: "played-desc" })).toString()).toBe("");
  });
});

describe("playerResult", () => {
  it("maps the result to the player's point of view", () => {
    expect(playerResult("1-0", "black")).toBe("loss");
    expect(playerResult("1-0", "white")).toBe("win");
    expect(playerResult("0-1", "black")).toBe("win");
    expect(playerResult("1/2-1/2", "white")).toBe("draw");
    expect(playerResult("*", "white")).toBe("unfinished");
  });
});

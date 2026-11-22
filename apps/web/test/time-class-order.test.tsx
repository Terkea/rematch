// @vitest-environment jsdom
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TimeClass } from "../src/server/games/types";

// I8, the owner's ruling: every time-control list on every page starts with Rapid, then Blitz,
// then Bullet, Classical and Daily, and Rapid is the default whenever the player has rapid games.
// One helper holds the order (`src/lib/time-class.ts`); this file checks that nothing else does.

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }),
}));

const { TIME_CLASS_ORDER, byTimeClass, defaultTimeClass } = await import(
  "../src/lib/time-class.js"
);
const { CoachScopeSwitch } = await import("../src/components/coach-scope-switch.js");

const SRC = join(dirname(fileURLToPath(import.meta.url)), "../src");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : /\.tsx?$/.test(name) ? [path] : [];
  });
}

describe("the time-control order (I8)", () => {
  it("is rapid, blitz, bullet, classical, daily", () => {
    expect(TIME_CLASS_ORDER).toEqual(["rapid", "blitz", "bullet", "classical", "daily"]);
    const mixed: TimeClass[] = ["daily", "bullet", "rapid", "classical", "blitz"];
    expect(mixed.sort(byTimeClass)).toEqual([...TIME_CLASS_ORDER]);
  });

  it("defaults to rapid when there is rapid, else the first class there is", () => {
    expect(defaultTimeClass(["blitz", "bullet", "rapid"])).toBe("rapid");
    expect(defaultTimeClass(["daily", "blitz"])).toBe("blitz");
    expect(defaultTimeClass([])).toBeNull();
  });

  it("a long Games coached switch shows the chosen rapid scope", () => {
    const account = {
      accountId: "22222222-2222-4222-8222-222222222222",
      platform: "chesscom" as const,
      username: "knightly",
    };
    render(
      <CoachScopeSwitch
        basePath="/app/report"
        scope={{
          ...account,
          timeClass: "rapid",
          options: [
            { ...account, timeClass: "blitz", analysedGames: 356 },
            { ...account, timeClass: "daily", analysedGames: 2 },
            { ...account, timeClass: "rapid", analysedGames: 159 },
            { ...account, timeClass: "bullet", analysedGames: 40 },
          ],
        }}
      />,
    );
    // Four options: the switch is a select; its trigger shows the chosen (rapid) option.
    const trigger = screen.getByRole("combobox", { name: "Games coached" });
    expect(trigger).toHaveTextContent("Rapid on chess.com (159 games)");
  });

  it("orders the segmented switch rapid first", () => {
    const account = {
      accountId: "22222222-2222-4222-8222-222222222222",
      platform: "chesscom" as const,
      username: "knightly",
    };
    render(
      <CoachScopeSwitch
        basePath="/app/weaknesses"
        scope={{
          ...account,
          timeClass: "rapid",
          options: [
            { ...account, timeClass: "blitz", analysedGames: 356 },
            { ...account, timeClass: "rapid", analysedGames: 159 },
          ],
        }}
      />,
    );
    expect(screen.getAllByRole("radio").map((r) => r.textContent)).toEqual([
      "Rapid on chess.com (159 games)Rapid on chess.com (159 games)",
      "Blitz on chess.com (356 games)Blitz on chess.com (356 games)",
    ]);
  });

  it("no page lists the time classes in an order of its own", () => {
    const own =
      /\[\s*"(?:bullet|blitz|rapid|classical|daily)",\s*"(?:bullet|blitz|rapid|classical|daily)"/;
    const labels = /label: "(?:Bullet|Blitz|Rapid|Classical|Daily)"/;
    const offenders = files(SRC)
      .filter((path) => !path.endsWith(join("lib", "time-class.ts")))
      // API request schemas validate a value, and the styleguide demos a control with made-up
      // options; neither orders anything about a player's games.
      .filter((path) => !relative(SRC, path).startsWith(join("app", "api")))
      .filter((path) => !relative(SRC, path).startsWith(join("app", "styleguide")))
      .filter((path) => {
        const text = readFileSync(path, "utf8");
        return own.test(text) || labels.test(text);
      })
      .map((path) => relative(SRC, path));
    expect(offenders).toEqual([]);
  });
});

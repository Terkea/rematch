// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import LicensesPage, { metadata } from "../src/app/licenses/page.js";

// /licenses (T11 hand-off to T27, ADR 0039 and its addendum): the Cburnett BSD-3-Clause notice
// with its attribution, covering both our rendered set and the copy bundled in react-chessboard,
// plus every shipped font, icon and data licence. Paths are from apps/web (vitest's cwd).

const text = () => document.body.textContent ?? "";
const squash = (s: string) => s.replace(/\s+/g, " ").trim();

describe("the licences page", () => {
  it("is titled", () => {
    expect(metadata.title).toBe("Licences");
  });

  it("carries the Cburnett BSD-3-Clause notice word for word, with the attribution", () => {
    render(<LicensesPage />);
    expect(screen.getByRole("heading", { level: 1, name: "Licences" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Chess pieces: Cburnett" })).toBeInTheDocument();
    const license = readFileSync(
      resolve("../../packages/ui/assets/pieces/cburnett/LICENSE"),
      "utf8",
    );
    const notice = license.slice(license.indexOf("Copyright (c) 2006, Colin M.L. Burnett"));
    expect(squash(text())).toContain(squash(notice));
    expect(text()).toContain("Colin M.L. Burnett");
    expect(text()).toContain("Wikimedia Commons");
    // The addendum: the copy react-chessboard bundles is covered too.
    expect(text()).toContain("react-chessboard");
  });

  it("lists the fonts with their OFL texts, the icons and the opening names", () => {
    render(<LicensesPage />);
    for (const font of ["Literata", "Schibsted-Grotesk"]) {
      const ofl = readFileSync(resolve(`src/app/fonts/${font.replace("-", "")}-OFL.txt`), "utf8");
      expect(squash(text())).toContain(squash(ofl));
    }
    expect(text()).toContain("Phosphor Icons");
    expect(text()).toContain("MIT License");
    expect(text()).toContain("lichess-org/chess-openings");
    expect(text()).toContain("CC0 1.0");
  });

  it("carries the shipped code's licences word for word (review-t27 M2)", () => {
    render(<LicensesPage />);
    for (const file of [
      "../../packages/ui/node_modules/@phosphor-icons/react/LICENSE",
      "node_modules/chess.js/LICENSE",
      "node_modules/react-chessboard/LICENSE",
    ]) {
      expect(squash(text())).toContain(squash(readFileSync(resolve(file), "utf8")));
    }
    expect(text()).toContain("Stockfish");
    expect(text()).toContain("Maia-2");
  });
});

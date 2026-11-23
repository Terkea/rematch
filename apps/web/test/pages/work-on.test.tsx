// @vitest-environment jsdom
import { weaknessRateText } from "@ci/ui/profile";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WorkOn } from "../../src/app/(app)/app/_overview/headline.js";
import type { CoachScope, LeadFinding, WeaknessSummary } from "../../src/server/profile/types.js";

// The overview's "What to work on" slot for each `LeadFinding` kind (synthetic values).

const scope: CoachScope = {
  accountId: "11111111-1111-4111-8111-111111111111",
  timeClass: "rapid",
  platform: "lichess",
  username: "synthetic",
  options: [
    {
      accountId: "11111111-1111-4111-8111-111111111111",
      timeClass: "rapid",
      platform: "lichess",
      username: "synthetic",
      analysedGames: 214,
    },
  ],
};

const weakness: WeaknessSummary = {
  seriesKey: "W3.shallow",
  contractId: "W3",
  label: "Missed short tactics",
  area: "tactics",
  state: "active",
  rate: { mean: 0.042, low: 0.031, high: 0.056, level: 0.8 },
  evidence: { events: 38, chances: 902, minEvidence: 40, sufficient: true, needMore: null },
  band: { median: 0.03, p40: 0.028 },
  compared: true,
  priority: 3.2,
  confidence: 0.8,
  pImp: null,
  updatedAt: new Date("2026-11-20T00:00:00.000Z"),
  onset: null,
};

describe("WorkOn", () => {
  it("a weakness shows its label, rate, basis and link", () => {
    const lead: LeadFinding = { kind: "weakness", scope, weakness };
    const { container } = render(<WorkOn lead={lead} />);
    const link = screen.getByRole("link", { name: /Missed short tactics/ });
    expect(link.getAttribute("href")).toBe(
      `/app/weaknesses/W3.shallow?account=${scope.accountId}&tc=rapid`,
    );
    expect(container).toHaveTextContent(
      weaknessRateText(weakness.rate, weakness.evidence, "in the last 6 months"),
    );
    expect(container).toHaveTextContent("Based on 214 analysed rapid games on lichess");
  });

  // M6: on the overview's All scope the slot said "across 356 analysed games", which were blitz's.
  // It names its own scope: the time class and the platform.
  it("none says nothing stands out, with its own scope's games counted", () => {
    render(<WorkOn lead={{ kind: "none", scope, analysedGames: 214, compared: true }} />);
    expect(
      screen.getByText(
        "Nothing stands out above players at your level across 214 analysed rapid games on lichess yet.",
      ),
    ).toBeInTheDocument();
  });

  // I1: with no benchmark nothing was compared, so "nothing stands out" overclaims.
  it("none without a benchmark says not compared yet", () => {
    const { container } = render(
      <WorkOn lead={{ kind: "none", scope, analysedGames: 214, compared: false }} />,
    );
    expect(container).toHaveTextContent(
      "Not compared yet: no benchmark for players at your level yet.",
    );
    expect(container).toHaveTextContent("214 analysed rapid games on lichess");
    expect(container).not.toHaveTextContent(/Nothing stands out/);
    const link = screen.getByRole("link", { name: /your weaknesses/i });
    expect(link.getAttribute("href")).toBe(`/app/weaknesses?account=${scope.accountId}&tc=rapid`);
  });

  // Critic B10: no coloured left border; red only on the finding's own words.
  it("sits on a sheet, never behind a coloured border", () => {
    const lead: LeadFinding = { kind: "weakness", scope, weakness };
    const { container } = render(<WorkOn lead={lead} />);
    const slot = container.querySelector("[data-slot='work-on']");
    expect(slot?.className).not.toMatch(/border-l|border-pen/);
    expect(slot?.getAttribute("data-surface")).toBe("sheet");
    expect(slot?.className).toMatch(/bg-sheet/);
  });

  it("building keeps the profile-building copy", () => {
    const { container } = render(<WorkOn lead={{ kind: "building" }} />);
    expect(container).toHaveTextContent("Your coaching profile is still being built.");
    expect(container.querySelector("a")).toBeNull();
  });
});

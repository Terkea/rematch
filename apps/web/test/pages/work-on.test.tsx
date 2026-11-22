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
    expect(container).toHaveTextContent(weaknessRateText(weakness.rate, weakness.evidence));
    expect(container).toHaveTextContent("Based on 214 analysed rapid games");
  });

  it("none says nothing stands out, with the games counted", () => {
    render(<WorkOn lead={{ kind: "none", scope, analysedGames: 214 }} />);
    expect(
      screen.getByText(
        "Nothing stands out above players at your level across 214 analysed games yet.",
      ),
    ).toBeInTheDocument();
  });

  it("building keeps the profile-building copy", () => {
    const { container } = render(<WorkOn lead={{ kind: "building" }} />);
    expect(container).toHaveTextContent("Your coaching profile is still being built.");
    expect(container.querySelector("a")).toBeNull();
  });
});

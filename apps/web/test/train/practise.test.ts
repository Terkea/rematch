import { describe, expect, it } from "vitest";
import { practiseLink } from "../../src/components/practise-positions";

// The weakness page's way into the trainer: a session of that source's positions only.

describe("practiseLink", () => {
  const scope = { accountId: "acc-1", timeClass: "rapid" as const };

  it("starts a session filtered to the weakness, naming the count", () => {
    expect(practiseLink("W4.missed", scope, 18)).toEqual({
      href: "/app/train?account=acc-1&tc=rapid&source=W4.missed",
      label: "Practise these 18 positions",
    });
    expect(practiseLink("W3.shallow", scope, 1)?.label).toBe("Practise this position");
  });

  it("is nothing for a weakness the trainer does not drill, or with no positions", () => {
    expect(practiseLink("W9a", scope, 5)).toBeNull();
    expect(practiseLink("W4.missed", scope, 0)).toBeNull();
  });
});

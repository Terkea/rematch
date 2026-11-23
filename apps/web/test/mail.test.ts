import { beforeEach, describe, expect, it, vi } from "vitest";

const sent: unknown[] = [];
vi.mock("nodemailer", () => ({
  default: {
    createTransport: () => ({
      sendMail: async (msg: unknown) => {
        sent.push(msg);
      },
    }),
  },
}));

const { sendMail, magicLinkEmail } = await import("../src/server/mail");

beforeEach(() => {
  sent.length = 0;
});

describe("sendMail, the one mail path (review n4)", () => {
  it("never sends to a Lichess placeholder address", async () => {
    await sendMail(magicLinkEmail("fakeuser@lichess.chess-improver.invalid", "http://x/y"));
    await sendMail(magicLinkEmail("FAKEUSER@Lichess.Chess-Improver.Invalid", "http://x/y"));
    expect(sent).toEqual([]);
  });

  it("sends to a real address", async () => {
    await sendMail(magicLinkEmail("ana@example.com", "http://x/y"));
    expect(sent).toHaveLength(1);
  });
});

describe("the sign-in email", () => {
  it("opens with the Rematch lockup, hosted on the app's own origin, with alt text", () => {
    const msg = magicLinkEmail("ana@example.com", "https://rematch.example/api/auth/magic?t=1");
    expect(msg.html).toMatch(
      /^<p><img src="https:\/\/rematch\.example\/brand\/rematch-lockup-mail\.png" width="220" height="76" alt="Rematch, chess improver"/,
    );
    // The plain-text part stays plain.
    expect(msg.text).not.toContain("<img");
  });
});

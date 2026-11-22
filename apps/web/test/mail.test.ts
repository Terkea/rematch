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

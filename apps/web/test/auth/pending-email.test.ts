import { NextRequest } from "next/server";
import { describe, expect, it } from "vitest";
import { proxy } from "../../src/proxy";
import {
  PENDING_EMAIL_COOKIE,
  PENDING_EMAIL_HEADER,
  pendingEmailFrom,
  withPendingEmail,
} from "../../src/server/auth/pending-email";

// Review I1: the address a link went to never travels in a URL. A successful send sets a short,
// HttpOnly cookie scoped to /sign-in/check-email; the proxy hands it to that page once and
// clears it.

const send = (body: unknown, path = "/api/auth/sign-in/magic-link") =>
  new Request(`http://localhost:3100${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
const ok = () => Response.json({ status: true });

describe("withPendingEmail", () => {
  it("sets a short HttpOnly cookie scoped to the check-email page after a send", async () => {
    const res = await withPendingEmail(send({ email: " Adult@Example.invalid " }), ok(), false);
    const cookie = res.headers.get("set-cookie") ?? "";
    expect(cookie).toContain(`${PENDING_EMAIL_COOKIE}=adult%40example.invalid`);
    expect(cookie).toContain("Max-Age=600");
    expect(cookie).toContain("Path=/sign-in/check-email");
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Lax");
    expect(cookie).not.toContain("Secure");
    expect(await res.json()).toEqual({ status: true });
  });

  it("marks the cookie Secure over https", async () => {
    const res = await withPendingEmail(send({ email: "a@example.invalid" }), ok(), true);
    expect(res.headers.get("set-cookie")).toContain("Secure");
  });

  it("sets nothing for a refused send, another route or something that isn't an address", async () => {
    const limited = Response.json({ code: "TOO_MANY" }, { status: 429 });
    for (const res of [
      await withPendingEmail(send({ email: "a@example.invalid" }), limited, false),
      await withPendingEmail(
        send({ email: "a@example.invalid" }, "/api/auth/sign-out"),
        ok(),
        false,
      ),
      await withPendingEmail(send({ email: "<b>x</b> urgent" }), ok(), false),
      await withPendingEmail(send({}), ok(), false),
    ]) {
      expect(res.headers.get("set-cookie")).toBeNull();
    }
  });

  it("accepts only an address-shaped value back", () => {
    expect(pendingEmailFrom("adult%40example.invalid")).toBe("adult@example.invalid");
    expect(pendingEmailFrom("%3Cb%3Ex%3C%2Fb%3E%40evil.example")).toBeNull();
    expect(pendingEmailFrom(undefined)).toBeNull();
    expect(pendingEmailFrom("%E0%A4%A")).toBeNull();
  });
});

describe("proxy on /sign-in/check-email", () => {
  it("hands the address to the page once and clears the cookie", () => {
    const req = new NextRequest("http://localhost:3100/sign-in/check-email", {
      headers: { cookie: `${PENDING_EMAIL_COOKIE}=adult%40example.invalid` },
    });
    const res = proxy(req);
    expect(res.headers.get(`x-middleware-request-${PENDING_EMAIL_HEADER}`)).toBe(
      "adult@example.invalid",
    );
    const cleared = res.headers.get("set-cookie") ?? "";
    expect(cleared).toContain(`${PENDING_EMAIL_COOKIE}=`);
    expect(cleared).toContain("Path=/sign-in/check-email");
    expect(cleared).toMatch(/Max-Age=0|Expires=Thu, 01 Jan 1970/);
  });

  it("drops a header the browser sent itself", () => {
    const req = new NextRequest("http://localhost:3100/sign-in/check-email", {
      headers: { [PENDING_EMAIL_HEADER]: "spoof@evil.example" },
    });
    const res = proxy(req);
    expect(res.headers.get(`x-middleware-request-${PENDING_EMAIL_HEADER}`)).toBeNull();
    // The forwarded request headers are always rewritten, without the browser's copy.
    const forwarded = res.headers.get("x-middleware-override-headers");
    expect(forwarded).not.toBeNull();
    expect(forwarded?.split(",")).not.toContain(PENDING_EMAIL_HEADER);
  });
});

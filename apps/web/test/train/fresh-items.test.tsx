// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh }),
}));

const { FreshItems } = await import("../../src/components/train/fresh-items.js");

// The trainer's new positions (ADR 0090 Amendment 1): the page never writes, so a stale page
// asks for them with a POST and reloads; the session is never hidden once shown.

const fetchMock = vi.fn();

beforeEach(() => {
  refresh.mockReset();
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const view = (stale: boolean) => (
  <FreshItems stale={stale} accountId="acc" timeClass="rapid" pending={<p>making</p>}>
    <p>session</p>
  </FreshItems>
);

describe("FreshItems", () => {
  it("shows the page as it is, and posts nothing, when nothing is waiting", () => {
    render(view(false));
    expect(screen.getByText("session")).toBeTruthy();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("posts the refresh while showing the placeholder, then reloads", async () => {
    fetchMock.mockResolvedValue(new Response(JSON.stringify({ added: 3 }), { status: 200 }));
    render(view(true));
    expect(screen.getByText("making")).toBeTruthy();
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("/api/train/refresh");
    expect(init).toMatchObject({ method: "POST" });
    expect(JSON.parse(String(init?.body))).toEqual({ accountId: "acc", timeClass: "rapid" });
  });

  it("shows what it has when the refresh fails", async () => {
    fetchMock.mockResolvedValue(new Response("{}", { status: 500 }));
    render(view(true));
    await waitFor(() => expect(screen.getByText("session")).toBeTruthy());
    expect(refresh).not.toHaveBeenCalled();
  });

  it("never hides a session already shown when new games land", async () => {
    fetchMock.mockResolvedValue(new Response("{}", { status: 200 }));
    const { rerender } = render(view(false));
    rerender(view(true));
    expect(screen.getByText("session")).toBeTruthy();
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
  });
});

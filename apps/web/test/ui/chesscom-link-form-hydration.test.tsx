// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, waitFor } from "@testing-library/react";
import { hydrateRoot } from "react-dom/client";
import { renderToString } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ChesscomLinkForm } from "../../src/app/(app)/app/accounts/chesscom-link-form.js";
import { stubFetch, stubMatchMedia } from "./helpers.js";

// Holistic F9: text typed into the username field before the page hydrated was wiped. The
// field keeps what the browser holds across hydration, and the submit reads it.

beforeEach(() => stubMatchMedia());
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("ChesscomLinkForm across hydration", () => {
  it("keeps and submits a username typed before hydration", async () => {
    const calls = stubFetch([
      {
        method: "POST",
        path: /\/api\/accounts\/chesscom/,
        status: 201,
        body: { linkId: "l-1", username: "mtsgambit", code: "CI-ABC234", verified: false },
      },
    ]);
    const onLinked = vi.fn();
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    const ui = (
      <QueryClientProvider client={client}>
        <ChesscomLinkForm onLinked={onLinked} />
      </QueryClientProvider>
    );
    const container = document.createElement("div");
    container.innerHTML = renderToString(ui);
    document.body.append(container);
    const input = container.querySelector("input[name='username']") as HTMLInputElement;
    input.value = "mtsgambit";
    await act(async () => {
      hydrateRoot(container, ui);
    });
    const field = container.querySelector("input[name='username']") as HTMLInputElement;
    expect(field.value).toBe("mtsgambit");
    await act(async () => {
      field.form?.requestSubmit();
    });
    await waitFor(() => expect(onLinked).toHaveBeenCalled());
    expect(calls.find((c) => c.method === "POST")?.body).toEqual({ username: "mtsgambit" });
    expect(container.textContent).not.toContain("Enter your chess.com username.");
  });
});

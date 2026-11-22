// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The thread hook (Task 9): the answer streams in from `chat.*` events applied to the query
// cache (`applyCoachEvent`), the stored thread replaces the stream when it ends, and a failure
// reads the server's copy. Synthetic rows only.

vi.mock("@ci/ui/overlays", () => ({ toast: vi.fn() }));

const { applyCoachEvent } = await import("../../src/lib/realtime/coach-events.js");
const { useThread } = await import("../../src/app/(app)/app/coach/[threadId]/use-thread.js");

const ACCOUNT = "00000000-0000-4000-8000-0000000000a1";
const THREAD = "00000000-0000-4000-8000-0000000000e1";
const USER_MSG = "00000000-0000-4000-8000-000000000101";
const ANSWER = "00000000-0000-4000-8000-000000000102";

const text = (t: string) => [{ kind: "text" as const, text: t }];

function message(over: Record<string, unknown>) {
  return {
    id: ANSWER,
    seq: 2,
    role: "assistant",
    status: "pending",
    anchor: null,
    segments: null,
    failureReason: null,
    aiDisclosure: null,
    createdAt: "2026-11-20T10:00:00.000Z",
    streamed: [],
    engineChecks: [],
    ...over,
  };
}

function thread(answer: Record<string, unknown> = {}) {
  return {
    id: THREAD,
    accountId: ACCOUNT,
    timeClass: "rapid" as const,
    title: "Chat with your coach",
    anchor: null,
    messages: [
      message({
        id: USER_MSG,
        seq: 1,
        role: "user",
        status: "done",
        segments: text("Why did I lose the endgame?"),
      }),
      message(answer),
    ],
    // biome-ignore lint/suspicious/noExplicitAny: the hook takes the server's Thread shape
  } as any;
}

const DISCLOSURE = {
  label: "Written by AI",
  note: "Test note.",
  model: "test-model",
  promptVersion: "chat@1",
  generatedAt: "2026-11-20T10:00:00.000Z",
};

let qc: QueryClient;
let id = 0;
const ev = (type: string, payload: unknown) => ({ id: ++id, accountId: ACCOUNT, type, payload });
const chunk = (seq: number, t: string) =>
  ev("chat.chunk", { messageId: ANSWER, seq, text: t, segments: text(t), interim: false });

const fetchMock = vi.fn();

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

const respond = (body: unknown, status = 200) =>
  Promise.resolve(new Response(JSON.stringify(body), { status }));

beforeEach(() => {
  qc = new QueryClient();
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

const answer = (r: { current: ReturnType<typeof useThread> }) =>
  r.current.messages.find((m) => m.id === ANSWER);

describe("useThread", () => {
  it("chunks stream into the pending answer", () => {
    const { result } = renderHook(() => useThread(THREAD, thread()), { wrapper });
    expect(answer(result)?.status).toBe("pending");
    act(() => {
      applyCoachEvent(qc, chunk(1, "Your rook left the seventh rank."));
    });
    expect(answer(result)?.status).toBe("streaming");
    expect(answer(result)?.streamed).toEqual([
      { seq: 1, segments: text("Your rook left the seventh rank."), interim: false },
    ]);
    // A second chunk joins in seq order, a replayed one changes nothing.
    act(() => {
      applyCoachEvent(qc, chunk(3, " Keep it active."));
      applyCoachEvent(qc, chunk(1, "Your rook left the seventh rank."));
    });
    expect(answer(result)?.streamed.map((c) => c.seq)).toEqual([1, 3]);
  });

  it("keeps the chunks stored before a reload and adds the live ones", () => {
    const initial = thread({
      status: "streaming",
      streamed: [{ seq: 1, segments: text("Stored part."), interim: false }],
    });
    const { result } = renderHook(() => useThread(THREAD, initial), { wrapper });
    act(() => {
      applyCoachEvent(qc, chunk(2, " Live part."));
    });
    expect(answer(result)?.streamed.map((c) => c.seq)).toEqual([1, 2]);
  });

  it("ready refetches and replaces the streamed text", async () => {
    fetchMock.mockImplementation(() =>
      respond(
        thread({ status: "done", segments: text("The checked answer."), aiDisclosure: DISCLOSURE }),
      ),
    );
    const { result } = renderHook(() => useThread(THREAD, thread()), { wrapper });
    act(() => {
      applyCoachEvent(qc, chunk(1, "A draft that was retracted."));
    });
    act(() => {
      applyCoachEvent(qc, ev("chat.message.ready", { messageId: ANSWER, replace: true }));
    });
    await waitFor(() => expect(answer(result)?.status).toBe("done"));
    expect(fetchMock).toHaveBeenCalledWith(
      `/api/coach/threads/${THREAD}`,
      expect.objectContaining({ cache: "no-store" }),
    );
    expect(answer(result)?.segments).toEqual(text("The checked answer."));
    expect(answer(result)?.streamed).toEqual([]);
    expect(answer(result)?.disclosure).toEqual(DISCLOSURE);
    expect(answer(result)?.settled).toBe(true);
  });

  it("failed shows the failure copy", async () => {
    // The refetch fails too: the stream's reason is still shown.
    fetchMock.mockImplementation(() => respond({ error: "x" }, 500));
    const { result } = renderHook(() => useThread(THREAD, thread()), { wrapper });
    act(() => {
      applyCoachEvent(
        qc,
        ev("chat.message.failed", { messageId: ANSWER, reason: "weekly_cap", text: "" }),
      );
    });
    await waitFor(() => expect(answer(result)?.status).toBe("failed"));
    expect(answer(result)?.failure).toBe(
      "You've used this week's coach messages. They reset on Monday.",
    );
    // A cap is not retried: asking again would only fail the same way.
    expect(answer(result)?.retry).toBeNull();
  });

  it("a stored failure reads its copy and an outage can be asked again", () => {
    const { result } = renderHook(
      () => useThread(THREAD, thread({ status: "failed", failureReason: "unavailable" })),
      { wrapper },
    );
    expect(answer(result)?.failure).toBe(
      "Your coach is unavailable right now. Please try again later.",
    );
    expect(answer(result)?.retry).toBe("Why did I lose the endgame?");
  });

  it("composer is disabled while answering", async () => {
    fetchMock.mockImplementation(() =>
      respond(thread({ status: "done", segments: text("Done."), aiDisclosure: DISCLOSURE })),
    );
    const { result } = renderHook(() => useThread(THREAD, thread()), { wrapper });
    expect(result.current.composer).toEqual({
      disabled: true,
      reason: "Your coach is answering",
    });
    act(() => {
      applyCoachEvent(qc, ev("chat.message.ready", { messageId: ANSWER, replace: false }));
    });
    await waitFor(() => expect(result.current.composer.disabled).toBe(false));
  });

  it("an exhausted allowance disables the composer with the cap copy", () => {
    const done = thread({ status: "done", segments: text("Done."), aiDisclosure: DISCLOSURE });
    const { result } = renderHook(() => useThread(THREAD, done, { exhausted: "weekly_cap" }), {
      wrapper,
    });
    expect(result.current.composer).toEqual({
      disabled: true,
      reason: "You've used this week's coach messages. They reset on Monday.",
    });
  });

  it("a post shows the question as sending, then the pending answer", async () => {
    const done = thread({ status: "done", segments: text("Done."), aiDisclosure: DISCLOSURE });
    let release: (r: Response) => void = () => {};
    fetchMock.mockImplementationOnce(
      () =>
        new Promise<Response>((r) => {
          release = r;
        }),
    );
    fetchMock.mockImplementation(() => respond({ error: "offline" }, 503));
    const { result } = renderHook(() => useThread(THREAD, done), { wrapper });
    let sent: Promise<boolean> = Promise.resolve(false);
    act(() => {
      sent = result.current.post("And the opening?");
    });
    const sending = result.current.messages.at(-1);
    expect(sending).toMatchObject({ role: "user", sending: true });
    expect(result.current.composer).toEqual({ disabled: true, reason: "Sending your question" });
    await act(async () => {
      release(
        new Response(
          JSON.stringify({
            userMessageId: "00000000-0000-4000-8000-000000000103",
            assistantMessageId: "00000000-0000-4000-8000-000000000104",
          }),
          { status: 202 },
        ),
      );
      await sent;
    });
    expect(await sent).toBe(true);
    const [q, a] = result.current.messages.slice(-2);
    expect(q).toMatchObject({ role: "user", sending: false });
    expect(a).toMatchObject({ role: "assistant", status: "pending" });
    expect(result.current.composer.reason).toBe("Your coach is answering");
  });

  it("a refused post keeps the question out of the thread and says why", async () => {
    const done = thread({ status: "done", segments: text("Done."), aiDisclosure: DISCLOSURE });
    fetchMock.mockImplementation(() => respond({ error: "busy" }, 409));
    const { result } = renderHook(() => useThread(THREAD, done), { wrapper });
    let ok = true;
    await act(async () => {
      ok = await result.current.post("Again?");
    });
    expect(ok).toBe(false);
    expect(result.current.messages).toHaveLength(2);
    expect(result.current.error).toBe(
      "Your coach is still answering your last question. Ask again once it's done.",
    );
  });
});

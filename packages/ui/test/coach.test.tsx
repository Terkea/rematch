import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AiDisclosure,
  ChatComposer,
  ChatMessage,
  type ChatMessageProps,
  CoachState,
  type CoachStateKind,
  engineLine,
  type Segment,
  SegmentText,
} from "../src/coach/index.js";

afterEach(cleanup);

const BAD = /NaN|undefined|Infinity|null/;

const disclosure = {
  label: "Written by AI",
  note: "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.",
  model: "claude-sonnet-5-5",
  promptVersion: "chat@1",
  generatedAt: "2026-11-12T08:30:00.000Z",
};

const every: Segment[] = [
  { kind: "text", text: "In your loss you played " },
  { kind: "move", gameId: "g1", ply: 17, san: "Nd2" },
  { kind: "text", text: " where " },
  { kind: "best", gameId: "g1", ply: 17, san: "Qe2" },
  { kind: "text", text: " held. The engine line was " },
  { kind: "pv", gameId: "g1", ply: 17, side: "best", n: 0, san: "Qe2 Nf6 O-O" },
  { kind: "text", text: ". Across " },
  { kind: "num", path: "basis.analysedGames", value: 214, display: "214" },
  { kind: "text", text: " games since " },
  { kind: "date", path: "basis.from", date: "2026-11-12" },
  { kind: "text", text: ", the pawn on " },
  { kind: "square", square: "e4" },
  { kind: "text", text: " fell after " },
  { kind: "line", lineKey: "sicilian-najdorf", n: null, san: "1. e4 c5 2. Nf3 d6" },
  { kind: "text", text: ". " },
  { kind: "eng", evalId: "ev1", line: 0, n: null, text: "+0.3" },
];

describe("SegmentText", () => {
  it("renders every segment kind", () => {
    const { container } = render(<SegmentText segments={every} />);
    const text = container.textContent ?? "";
    for (const part of [
      "In your loss you played",
      "Nd2",
      "Qe2",
      "Qe2 Nf6 O-O",
      "214",
      "12 Nov 2026",
      "e4",
      "1. e4 c5 2. Nf3 d6",
      "+0.3",
    ]) {
      expect(text).toContain(part);
    }
    expect(container.querySelector("time")).toHaveAttribute("datetime", "2026-11-12");
    expect(container.innerHTML).not.toContain("{{");
    expect(container.innerHTML).not.toMatch(BAD);
    // best carries the good mark, and is named for assistive tech.
    const best = container.querySelector("[data-segment='best']");
    expect(best).toHaveAttribute("data-mark", "good");
    expect(best?.textContent).toContain("better move");
  });

  it("move segments link through hrefFor", () => {
    render(
      <SegmentText
        segments={[{ kind: "move", gameId: "g1", ply: 5, san: "d4" }]}
        hrefFor={(s) => (s.kind === "move" ? `/app/games/${s.gameId}?ply=${s.ply}` : null)}
      />,
    );
    const link = screen.getByRole("link", { name: "d4" });
    expect(link).toHaveAttribute("href", "/app/games/g1?ply=5");
  });

  it("does not link when hrefFor returns null", () => {
    render(
      <SegmentText
        segments={[{ kind: "move", gameId: "g1", ply: 5, san: "d4" }]}
        hrefFor={() => null}
      />,
    );
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("unknown segment kind renders nothing", () => {
    const segments = [
      { kind: "text", text: "Before." },
      { kind: "future" },
      null,
      { kind: "num", path: "x", value: Number.NaN },
      { kind: "date", path: "x", date: "not a date" },
      { kind: "text", text: "After." },
    ] as unknown as Segment[];
    const { container } = render(<SegmentText segments={segments} />);
    expect(container.textContent).toBe("Before.After.");
    expect(container.innerHTML).not.toMatch(BAD);
  });

  it("num without display falls back to the value, never NaN", () => {
    const segments = [{ kind: "num", path: "basis.analysedGames", value: 2140 }] as Segment[];
    const { container } = render(<SegmentText segments={segments} />);
    expect(container.textContent).toBe("2,140");
  });

  it("eng segment is marked as an engine check", () => {
    render(
      <SegmentText segments={[{ kind: "eng", evalId: "e", line: 0, n: null, text: "+0.3" }]} />,
    );
    const eng = screen.getByRole("note", { name: "Engine check: +0.3" });
    expect(eng).toHaveAttribute("data-mark", "pencil");
    expect(eng.textContent).toContain("engine check");
  });

  it("renders as a span when asked", () => {
    const { container } = render(
      <SegmentText as="span" segments={[{ kind: "text", text: "x" }]} />,
    );
    expect(container.firstElementChild?.tagName).toBe("SPAN");
  });
});

describe("AiDisclosure", () => {
  it("shows label and day-precision date", () => {
    const { container } = render(<AiDisclosure disclosure={disclosure} />);
    expect(screen.getByText("Written by AI")).toBeInTheDocument();
    expect(container.textContent).toContain("Written on 12 Nov 2026");
    expect(container.textContent).toContain("It can contain mistakes.");
    expect(container.querySelector("time")).toHaveAttribute("datetime", "2026-11-12");
    expect(container.innerHTML).not.toMatch(BAD);
  });

  it("compact keeps the note for assistive tech and behind a tooltip trigger", () => {
    const { container } = render(<AiDisclosure disclosure={disclosure} compact />);
    expect(container.textContent).toContain("Written on 12 Nov 2026");
    expect(screen.getByRole("button", { name: "About this AI text" })).toBeInTheDocument();
    expect(container.querySelector(".sr-only")?.textContent).toContain("It can contain mistakes.");
  });

  it("compact note opens on tap and the trigger has a 44px touch target", async () => {
    render(<AiDisclosure disclosure={disclosure} compact />);
    const trigger = screen.getByRole("button", { name: "About this AI text" });
    expect(trigger.className).toContain("pointer-coarse:after:min-h-11");
    expect(trigger.className).toContain("pointer-coarse:after:min-w-11");
    fireEvent.click(trigger);
    const popup = await screen.findByRole("dialog");
    expect(popup.textContent).toContain("It can contain mistakes.");
  });

  it("leaves out an unreadable date rather than printing it", () => {
    const { container } = render(
      <AiDisclosure disclosure={{ ...disclosure, generatedAt: "garbage" }} />,
    );
    expect(container.textContent).not.toContain("Written on");
    expect(container.innerHTML).not.toMatch(/NaN|Invalid/);
  });
});

describe("CoachState", () => {
  const kinds: CoachStateKind[] = [
    "building",
    "writing",
    "failed",
    "budget",
    "unavailable",
    "computed_only",
    "empty",
  ];

  it.each(kinds)("%s renders its title and body with no bad text", (kind) => {
    const { container } = render(
      <CoachState kind={kind} title={`Title ${kind}`} body="What happens next." />,
    );
    expect(container.textContent).toContain(`Title ${kind}`);
    expect(container.textContent).toContain("What happens next.");
    expect(container.innerHTML).not.toMatch(BAD);
    expect(container.innerHTML).not.toMatch(/animate-spin/);
  });

  it("building and writing use the progress placeholder", () => {
    const { container } = render(
      <CoachState
        kind="building"
        title="Building your report"
        body="Sections appear as they are written."
        progress={{ done: 2, total: 5, unit: "sections" }}
      />,
    );
    expect(container.querySelector("[data-shape]")).not.toBeNull();
    expect(container.textContent).toContain("2 / 5 sections");
  });

  it("building and writing name their own work, never Analysing", () => {
    const { container: b } = render(
      <CoachState
        kind="building"
        title="Building your report"
        progress={{ done: 2, total: 5, unit: "sections" }}
      />,
    );
    expect(b.querySelector("[data-part='label']")?.textContent).toBe("Building 2 / 5 sections");
    cleanup();
    const { container: w } = render(<CoachState kind="writing" title={"Your coach is reading"} />);
    expect(w.querySelector("[data-part='label']")?.textContent).toBe("Writing");
    expect(w.textContent).not.toContain("Analysing");
  });

  it("an action renders a button that calls back", async () => {
    const onClick = vi.fn();
    render(
      <CoachState
        kind="failed"
        title="The report could not be written"
        body="Try again in a few minutes."
        action={{ label: "Try again", onClick }}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("an action with an href renders a link", () => {
    render(
      <CoachState
        kind="empty"
        title="No questions yet"
        body="Ask about a game."
        action={{ label: "Open your games", href: "/app/games" }}
      />,
    );
    expect(screen.getByRole("link", { name: "Open your games" })).toHaveAttribute(
      "href",
      "/app/games",
    );
  });
});

const base: ChatMessageProps = {
  role: "assistant",
  status: "done",
  segments: null,
  streamed: [],
  notes: [],
  engineChecks: [],
  failure: null,
  disclosure: null,
};

describe("ChatMessage", () => {
  it("pending message shows a writing placeholder, not a spinner", () => {
    const { container } = render(<ChatMessage {...base} status="pending" />);
    expect(container.textContent).toContain("Your coach is reading your games");
    for (const bar of container.querySelectorAll("[role='progressbar']")) {
      expect(bar).toHaveAttribute("aria-valuenow");
    }
    expect(container.innerHTML).not.toMatch(/animate-spin/);
    expect(container.querySelector("[data-shape]")).not.toBeNull();
  });

  it("streamed chunks render in seq order", () => {
    const chunk = (seq: number, text: string) => ({
      seq,
      interim: false,
      segments: [{ kind: "text" as const, text }],
    });
    const { container } = render(
      <ChatMessage
        {...base}
        status="streaming"
        streamed={[chunk(2, "third."), chunk(0, "first "), chunk(1, "second ")]}
      />,
    );
    const body = container.querySelector("[data-part='streamed']");
    expect(body?.textContent).toContain("first second third.");
    // Visibly AI while it streams.
    expect(container.textContent).toContain("Written by AI");
  });

  it("an interim chunk is a pencil aside", () => {
    const { container } = render(
      <ChatMessage
        {...base}
        status="streaming"
        streamed={[
          { seq: 0, interim: false, segments: [{ kind: "text", text: "Let me check." }] },
          {
            seq: 1,
            interim: true,
            segments: [{ kind: "text", text: "Checking 1. e4 e5 with the engine" }],
          },
        ]}
      />,
    );
    const aside = container.querySelector("aside");
    expect(aside).toHaveAttribute("data-tone", "pencil");
    expect(aside?.textContent).toContain("Checking 1. e4 e5 with the engine");
  });

  it("done shows the stored segments, notes, engine checks and disclosure", () => {
    const { container } = render(
      <ChatMessage
        {...base}
        segments={[{ kind: "text", text: "Your knight belongs on f3." }]}
        streamed={[{ seq: 0, interim: false, segments: [{ kind: "text", text: "stale" }] }]}
        notes={["One statement was left out because the engine could not confirm it."]}
        engineChecks={[{ status: "done", moves: ["e4", "e5", "Nf3"], startPly: 1 }]}
        disclosure={disclosure}
      />,
    );
    const text = container.textContent ?? "";
    expect(text).toContain("Your knight belongs on f3.");
    expect(text).not.toContain("stale");
    expect(text).toContain("One statement was left out");
    expect(text).toContain("Engine check: 1. e4 e5 2. Nf3 (done)");
    expect(text).toContain("Written on 12 Nov 2026");
    expect(container.innerHTML).not.toMatch(BAD);
  });

  it("failed shows the failure copy", () => {
    const { container } = render(
      <ChatMessage
        {...base}
        status="failed"
        failure="Your coach could not answer. Ask again in a minute."
      />,
    );
    expect(container.textContent).toContain("Your coach could not answer.");
    expect(container.innerHTML).not.toMatch(BAD);
  });

  it("a user message shows the question", () => {
    const { container } = render(
      <ChatMessage
        {...{ ...base, role: "user" }}
        segments={[{ kind: "text", text: "Why did I lose?" }]}
      />,
    );
    expect(container.textContent).toContain("Why did I lose?");
    expect(container.textContent).not.toContain("Written by AI");
  });
});

describe("ChatComposer", () => {
  it("composer refuses empty and over-limit input", async () => {
    const onSubmit = vi.fn();
    render(<ChatComposer onSubmit={onSubmit} placeholder="Ask about your games" />);
    const send = screen.getByRole("button", { name: "Send" });
    expect(send).toBeDisabled();
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "   " } });
    expect(send).toBeDisabled();
    fireEvent.change(box, { target: { value: "x".repeat(2001) } });
    expect(send).toBeDisabled();
    expect(screen.getByText("1 character over")).toBeInTheDocument();
    fireEvent.keyDown(box, { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("counts down from 200 left", () => {
    render(<ChatComposer onSubmit={() => {}} placeholder="Ask" />);
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "x".repeat(1700) } });
    expect(screen.queryByText(/characters left/)).toBeNull();
    fireEvent.change(box, { target: { value: "x".repeat(1801) } });
    expect(screen.getByText("199 characters left")).toBeInTheDocument();
  });

  it("composer sends on Enter, breaks on Shift+Enter", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();
    render(<ChatComposer onSubmit={onSubmit} placeholder="Ask" />);
    const box = screen.getByRole("textbox");
    await user.click(box);
    await user.keyboard("Why{Shift>}{Enter}{/Shift}now");
    expect(onSubmit).not.toHaveBeenCalled();
    expect(box).toHaveValue("Why\nnow");
    await user.keyboard("{Enter}");
    expect(onSubmit).toHaveBeenCalledWith("Why\nnow");
    await act(async () => {});
    expect(box).toHaveValue("");
  });

  it("keeps the text when the submit is refused", async () => {
    const user = userEvent.setup();
    render(<ChatComposer onSubmit={() => false} placeholder="Ask" />);
    const box = screen.getByRole("textbox");
    await user.click(box);
    await user.keyboard("Hello{Enter}");
    await act(async () => {});
    expect(box).toHaveValue("Hello");
  });

  it("disabledReason replaces the send button", () => {
    const { container } = render(
      <ChatComposer
        onSubmit={() => {}}
        placeholder="Ask"
        disabled
        disabledReason="You have used this week's 3 questions. They reset on 16 Nov 2026."
      />,
    );
    expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
    expect(container.textContent).toContain("They reset on 16 Nov 2026.");
    expect(screen.getByRole("textbox")).toBeDisabled();
    expect(container.innerHTML).not.toMatch(BAD);
  });
});

describe("fix round 2", () => {
  const text = (t: string): Segment[] => [{ kind: "text", text: t }];

  it("a done answer without a stored disclosure still says it was written by AI", () => {
    const { container } = render(<ChatMessage {...base} segments={text("Play Nf3.")} />);
    expect(container.textContent).toContain("Written by AI");
  });

  it("the done answer is not re-announced by a live region", () => {
    const streamed = [{ seq: 0, interim: false, segments: text("Play Nf3 now.") }];
    const { container, rerender } = render(
      <ChatMessage {...base} status="streaming" streamed={streamed} />,
    );
    const liveNow = [...container.querySelectorAll("[aria-live='polite']")];
    expect(liveNow.some((el) => el.textContent?.includes("Play Nf3 now."))).toBe(true);
    rerender(
      <ChatMessage
        {...base}
        status="done"
        segments={text("Play Nf3 now.")}
        disclosure={disclosure}
      />,
    );
    const live = [...container.querySelectorAll("[aria-live='polite'], [aria-live='assertive']")];
    expect(live.some((el) => el.textContent?.includes("Play Nf3 now."))).toBe(false);
    expect(container.textContent).toContain("Play Nf3 now.");
  });

  it("engine checks are numbered from their real ply, never from 1", () => {
    expect(engineLine(["Nd2", "e4", "Nxe4"], 55)).toBe("28. Nd2 e4 29. Nxe4");
    expect(engineLine(["e4", "Nxe4"], 56)).toBe("28… e4 29. Nxe4");
    expect(engineLine(["Nd2", "e4"], null)).toBe("Nd2 e4");
    expect(engineLine(["Nd2", "e4"])).toBe("Nd2 e4");
  });

  it("done, pending and failed engine checks have distinct marks and labels", () => {
    const { container } = render(
      <ChatMessage
        {...base}
        segments={text("x")}
        engineChecks={[
          { status: "done", moves: ["Nd2", "e4"], startPly: 55 },
          { status: "pending", moves: ["Nd2", "Rf8"], startPly: 55 },
          { status: "failed", moves: ["Nd2", "g5"], startPly: 55 },
        ]}
      />,
    );
    const rows = [...container.querySelectorAll("[data-engine-check]")];
    expect(rows.map((r) => r.querySelector("[data-mark]")?.getAttribute("data-mark"))).toEqual([
      "done",
      "pending",
      "failed",
    ]);
    expect(rows[0]?.textContent).toBe("Engine check: 28. Nd2 e4 (done)");
    expect(rows[1]?.textContent).toBe("Engine check: 28. Nd2 Rf8 (checking)");
    expect(rows[2]?.textContent).toBe("Engine check: 28. Nd2 g5 (failed)");
  });

  it("building shows every section with the finished ones marked", () => {
    const items = ["Overview", "Weaknesses", "Openings", "Climb plan", "Areas"].map((label, i) => ({
      label,
      done: i < 2,
    }));
    const { container } = render(
      <CoachState
        kind="building"
        title="Building your report"
        progress={{ done: 2, total: 5, unit: "sections" }}
        items={items}
      />,
    );
    const rows = container.querySelectorAll("[data-row]");
    expect(rows).toHaveLength(5);
    expect(container.querySelectorAll("[data-row][data-done]")).toHaveLength(2);
    expect(rows[0]?.textContent).toContain("Overview");
    expect(rows[4]?.textContent).toContain("Areas");
  });

  it("links only to internal paths and http(s)", () => {
    const seg: Segment[] = [{ kind: "move", gameId: "g", ply: 1, san: "e4" }];
    for (const bad of ["javascript:alert(1)", "//evil.example/x", "data:text/html,x", " /x"]) {
      const { container } = render(<SegmentText segments={seg} hrefFor={() => bad} />);
      expect(container.querySelector("a")).toBeNull();
      cleanup();
    }
    const { container: ok } = render(<SegmentText segments={seg} hrefFor={() => "/app/g?ply=1"} />);
    expect(ok.querySelector("a")).toHaveAttribute("href", "/app/g?ply=1");
    cleanup();
    const { container: ext } = render(
      <SegmentText segments={seg} hrefFor={() => "https://lichess.org/abc"} />,
    );
    expect(ext.querySelector("a")).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("labels only the first engine check in a run of text", () => {
    const { container } = render(
      <SegmentText
        segments={[
          { kind: "eng", evalId: "a", line: 0, n: null, text: "+2.7" },
          { kind: "text", text: " to " },
          { kind: "eng", evalId: "b", line: 0, n: null, text: "−1.1" },
        ]}
      />,
    );
    expect(screen.getAllByRole("note")).toHaveLength(2);
    expect(container.textContent?.match(/engine check/g)).toHaveLength(1);
  });

  it("announces the count after typing pauses, not per keystroke", () => {
    vi.useFakeTimers();
    try {
      const { container } = render(<ChatComposer onSubmit={() => {}} placeholder="Ask" />);
      const box = screen.getByRole("textbox");
      fireEvent.change(box, { target: { value: "x".repeat(1801) } });
      const announce = container.querySelector("[data-part='count-announce']");
      expect(announce).toHaveAttribute("aria-live", "polite");
      expect(announce?.textContent).toBe("");
      act(() => {
        vi.advanceTimersByTime(1200);
      });
      expect(announce?.textContent).toBe("199 characters left");
      // The visible count is not itself a live region.
      expect(
        screen.getByText("199 characters left", { selector: ":not(.sr-only)" }),
      ).not.toHaveAttribute("aria-live");
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores the Enter that commits an IME composition (keyCode 229)", () => {
    const onSubmit = vi.fn();
    render(<ChatComposer onSubmit={onSubmit} placeholder="Ask" />);
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "日本" } });
    fireEvent.keyDown(box, { key: "Enter", keyCode: 229 });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("a used-up composer does not invite a question", () => {
    render(
      <ChatComposer
        onSubmit={() => {}}
        placeholder="Ask about your games"
        disabled
        disabledReason="You have used this week's 100 questions. They reset on 16 Nov 2026."
      />,
    );
    const box = screen.getByRole("textbox");
    expect(box).not.toHaveAttribute("placeholder", "Ask about your games");
    expect(box.getAttribute("placeholder")).toBe("Questions are paused until they reset");
  });

  it("duplicate notes both render", () => {
    const { container } = render(
      <ChatMessage {...base} segments={text("x")} notes={["Same note.", "Same note."]} />,
    );
    expect(container.textContent?.match(/Same note\./g)).toHaveLength(2);
  });
});

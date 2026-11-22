import "@testing-library/jest-dom/vitest";
import { cleanup, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, type Mock, vi } from "vitest";
import { Board } from "../src/board/index.js";
import { EvalGraph } from "../src/eval-graph/index.js";
import { games } from "../src/fixtures/index.js";
import { REVIEW_KEY_GUARD, type ReviewKeyAction, useReviewKeys } from "../src/review/index.js";

afterEach(() => {
  cleanup();
  for (const el of document.querySelectorAll("[data-fixture]")) el.remove();
});

/** Plain DOM outside React, standing in for the widgets the guard protects. */
function mount(html: string) {
  const container = document.createElement("div");
  container.dataset.fixture = "";
  container.innerHTML = html;
  document.body.append(container);
  return { container };
}

const ACTIONS: ReviewKeyAction[] = [
  "prev",
  "next",
  "first",
  "last",
  "prevMoment",
  "nextMoment",
  "flip",
  "play",
  "engine",
];

function spies() {
  return Object.fromEntries(ACTIONS.map((a) => [a, vi.fn<() => void>()])) as Record<
    ReviewKeyAction,
    Mock<() => void>
  >;
}

function press(target: EventTarget, key: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

const calls = (map: ReturnType<typeof spies>) =>
  ACTIONS.filter((a) => map[a].mock.calls.length > 0);

describe("useReviewKeys", () => {
  it("maps each key to its action", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const keys: [string, ReviewKeyAction][] = [
      ["ArrowLeft", "prev"],
      ["ArrowRight", "next"],
      ["Home", "first"],
      ["End", "last"],
      ["ArrowUp", "prevMoment"],
      ["ArrowDown", "nextMoment"],
      ["f", "flip"],
      [" ", "play"],
      ["b", "engine"],
    ];
    for (const [key, action] of keys) {
      press(document.body, key);
      expect(map[action], key).toHaveBeenCalledTimes(1);
    }
    expect(calls(map)).toEqual(ACTIONS);
  });

  it("calls next for ArrowRight on the body, and prevents the default scroll", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const e = press(document.body, "ArrowRight");
    expect(calls(map)).toEqual(["next"]);
    expect(e.defaultPrevented).toBe(true);
  });

  it("calls play for Space and prevents its default", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const e = press(document.body, " ", { code: "Space" });
    expect(calls(map)).toEqual(["play"]);
    expect(e.defaultPrevented).toBe(true);
  });

  it("ignores a key it has no action for, and leaves its default alone", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const e = press(document.body, "x");
    expect(calls(map)).toEqual([]);
    expect(e.defaultPrevented).toBe(false);
  });

  it("does nothing for a key whose action is not in the map", () => {
    const next = vi.fn();
    renderHook(() => useReviewKeys({ next }));
    const e = press(document.body, "ArrowLeft");
    expect(e.defaultPrevented).toBe(false);
    expect(next).not.toHaveBeenCalled();
  });

  it.each(["ctrlKey", "altKey", "metaKey"] as const)("ignores %s with an arrow", (mod) => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const e = press(document.body, "ArrowRight", { [mod]: true });
    expect(calls(map)).toEqual([]);
    expect(e.defaultPrevented).toBe(false);
  });

  it.each([
    ['<div role="slider" tabindex="0"><span id="t"></span></div>', "ArrowRight"],
    ['<div role="grid"><button id="t"></button></div>', "ArrowRight"],
    ['<div role="dialog"><button id="t"></button></div>', "End"],
    ['<input id="t" />', "f"],
    ['<textarea id="t"></textarea>', "b"],
    ['<select id="t"></select>', "ArrowDown"],
    ['<div contenteditable="true"><p id="t"></p></div>', "Home"],
  ])("ignores keys from inside %s", (html, key) => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const { container } = mount(html);
    const target = container.querySelector("#t") as HTMLElement;
    const e = press(target, key);
    expect(calls(map)).toEqual([]);
    expect(e.defaultPrevented).toBe(false);
  });

  it("ignores a key a focused widget already handled", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const { container } = mount('<div id="t" tabindex="0"></div>');
    const target = container.querySelector("#t") as HTMLElement;
    target.addEventListener("keydown", (e) => e.preventDefault());
    press(target, "ArrowRight");
    expect(calls(map)).toEqual([]);
  });

  it("leaves Space to a focused button, so it activates the button only", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const { container } = mount('<button id="t" type="button">Next move</button>');
    const e = press(container.querySelector("#t") as HTMLElement, " ");
    expect(calls(map)).toEqual([]);
    expect(e.defaultPrevented).toBe(false);
  });

  it("still steps with the arrows from a focused button", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const { container } = mount('<button id="t" type="button">Next move</button>');
    press(container.querySelector("#t") as HTMLElement, "ArrowRight");
    expect(calls(map)).toEqual(["next"]);
  });

  it("toggles once for a held Space, but repeats the arrows", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    press(document.body, " ");
    press(document.body, " ", { repeat: true });
    press(document.body, "ArrowRight");
    press(document.body, "ArrowRight", { repeat: true });
    expect(map.play).toHaveBeenCalledTimes(1);
    expect(map.next).toHaveBeenCalledTimes(2);
  });

  it("binds nothing with enabled false", () => {
    const map = spies();
    renderHook(() => useReviewKeys(map, { enabled: false }));
    for (const key of ["ArrowRight", " ", "f", "End"]) press(document.body, key);
    expect(calls(map)).toEqual([]);
  });

  it("unbinds on unmount and when disabled", () => {
    const map = spies();
    const { rerender, unmount } = renderHook(({ enabled }) => useReviewKeys(map, { enabled }), {
      initialProps: { enabled: true },
    });
    rerender({ enabled: false });
    press(document.body, "ArrowRight");
    expect(map.next).not.toHaveBeenCalled();
    rerender({ enabled: true });
    press(document.body, "ArrowRight");
    expect(map.next).toHaveBeenCalledTimes(1);
    unmount();
    press(document.body, "ArrowRight");
    expect(map.next).toHaveBeenCalledTimes(1);
  });

  it("calls the latest handler without rebinding", () => {
    const add = vi.spyOn(window, "addEventListener");
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(({ next }) => useReviewKeys({ next }), {
      initialProps: { next: first },
    });
    const bound = add.mock.calls.filter(([type]) => type === "keydown").length;
    rerender({ next: second });
    press(document.body, "ArrowRight");
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    expect(add.mock.calls.filter(([type]) => type === "keydown")).toHaveLength(bound);
    add.mockRestore();
  });
});

/** An element matching one guard selector, with a focusable child as the key's target. */
function guarded(selector: string): HTMLElement {
  const tag = /^[a-z]+$/.exec(selector)?.[0];
  if (tag) {
    const el = document.createElement(tag);
    el.id = "t";
    return el;
  }
  const outer = document.createElement("div");
  const role = /^\[role="([a-z]+)"\]$/.exec(selector)?.[1];
  if (role) outer.setAttribute("role", role);
  else if (selector.startsWith("[contenteditable]")) outer.setAttribute("contenteditable", "true");
  else throw new Error(`no builder for ${selector}`);
  const inner = document.createElement("span");
  inner.id = "t";
  outer.append(inner);
  return outer;
}

const GUARD = REVIEW_KEY_GUARD.split(",");
const ALL_KEYS = ["ArrowLeft", "ArrowRight", "Home", "End", "ArrowUp", "ArrowDown", "f", " ", "b"];

describe("the guard list", () => {
  it("is exactly the brief's selectors plus the text-entry and composite roles", () => {
    expect([...GUARD].sort()).toEqual(
      [
        "input",
        "textarea",
        "select",
        "[contenteditable]:not([contenteditable='false'])",
        '[role="grid"]',
        '[role="slider"]',
        '[role="dialog"]',
        '[role="textbox"]',
        '[role="searchbox"]',
        '[role="spinbutton"]',
        '[role="combobox"]',
        '[role="listbox"]',
        '[role="menu"]',
        '[role="menubar"]',
        '[role="tablist"]',
        '[role="radiogroup"]',
        '[role="toolbar"]',
        '[role="tree"]',
        '[role="treegrid"]',
      ].sort(),
    );
  });

  it.each(GUARD)("ignores every review key from inside %s", (selector) => {
    const map = spies();
    renderHook(() => useReviewKeys(map));
    const el = guarded(selector);
    const container = document.createElement("div");
    container.dataset.fixture = "";
    container.append(el);
    document.body.append(container);
    expect(el.matches(selector)).toBe(true);
    const target = container.querySelector("#t") as HTMLElement;
    for (const key of ALL_KEYS) press(target, key);
    expect(calls(map)).toEqual([]);
  });
});

const game = games.middlegameBlunder;
const points = game.plies.map((p) => ({
  ply: p.ply,
  winWhite: p.winWhite,
  san: p.san,
  judgement: p.judgement,
}));

function Page({ map }: { map: Partial<Record<ReviewKeyAction, () => void>> }) {
  useReviewKeys(map);
  return (
    <>
      <Board fen={game.startFen} label="Position" />
      <EvalGraph points={points} label="Win chance" currentPly={10} onSelectPly={() => {}} />
    </>
  );
}

/** The real widgets, as on the review page. */
describe("with the real Board and EvalGraph", () => {
  class FixedResizeObserver {
    constructor(private readonly callback: ResizeObserverCallback) {}
    observe(target: Element) {
      const rect = { width: 320, height: 0, top: 0, left: 0, right: 320, bottom: 0, x: 0, y: 0 };
      this.callback(
        [{ target, contentRect: rect } as unknown as ResizeObserverEntry],
        this as unknown as ResizeObserver,
      );
    }
    unobserve() {}
    disconnect() {}
  }

  it("leaves a focused square's arrows to the board, but flips with f", () => {
    vi.stubGlobal("ResizeObserver", FixedResizeObserver);
    try {
      const map = spies();
      render(<Page map={map} />);
      const e2 = screen.getByRole("button", { name: /^e2,/ });
      e2.focus();
      press(e2, "ArrowRight");
      press(e2, "ArrowUp");
      expect(calls(map)).toEqual([]);
      press(document.activeElement as HTMLElement, "f");
      expect(calls(map)).toEqual(["flip"]);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("leaves the slider's arrows to the graph", () => {
    vi.stubGlobal("ResizeObserver", FixedResizeObserver);
    try {
      const map = spies();
      render(<Page map={map} />);
      const slider = screen.getByRole("slider");
      slider.focus();
      for (const key of ["ArrowLeft", "ArrowRight", "Home", "End"]) press(slider, key);
      expect(calls(map)).toEqual([]);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

import "@testing-library/jest-dom/vitest";
import { act, cleanup, render } from "@testing-library/react";
import type { Ref } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Stat } from "../src/stats/index.js";

// jsdom never upgrades NumberFlow's custom element, so a value change throws inside its snapshot
// hook. NumberFlow becomes its raw value here; the ref still reaches the element.
vi.mock("@number-flow/react", () => ({
  default: ({ value, ref }: { value: number; ref?: Ref<HTMLSpanElement> }) => (
    <span ref={ref}>{value}</span>
  ),
}));

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Stat first-view roll", () => {
  it("rolls up from zero when its reveal wrapper is still hidden", async () => {
    vi.useFakeTimers();
    render(
      <div data-reveal-fig style={{ opacity: 0 }}>
        <Stat value={0.661} format="percent" rollIn rollDelay={300} />
      </div>,
    );
    await act(async () => {});
    expect(document.body.textContent).toBe("0");
    await act(async () => {
      vi.advanceTimersByTime(400);
    });
    expect(document.body.textContent).toContain("0.661");
  });

  it("shows the value straight away when the wrapper is already visible", async () => {
    render(
      <div data-reveal-fig>
        <Stat value={0.661} format="percent" rollIn rollDelay={300} />
      </div>,
    );
    await act(async () => {});
    expect(document.body.textContent).toBe("0.661");
  });
});

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import {
  type ExplainEntry,
  ExplainPopover,
  KpiTrendGrid,
  Meaning,
  WeaknessRow,
} from "../src/profile/index.js";

afterEach(cleanup);

const entry: ExplainEntry = {
  name: "Not winning won games",
  meaning: "You had a winning position and did not turn it into a win.",
  measured: "Chances: games where your win% was 85 or more before 3 of your moves in a row.",
  whyItMatters: "Every won game you fail to win costs rating you had already earned.",
  tip: "When you are winning, trade pieces and stop your opponent's counterplay.",
};

describe("ExplainPopover", () => {
  it("is a button named after the term, collapsed until pressed", () => {
    render(<ExplainPopover entry={entry} />);
    const trigger = screen.getByRole("button", { name: "What is not winning won games?" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("opens on click with the meaning, how it is measured, why it matters and the tip", async () => {
    const user = userEvent.setup();
    render(<ExplainPopover entry={entry} />);
    const trigger = screen.getByRole("button", { name: /What is/ });
    await user.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent(entry.meaning);
    expect(dialog).toHaveTextContent("How it's measured");
    expect(dialog).toHaveTextContent(entry.measured);
    expect(dialog).toHaveTextContent("Why it matters");
    expect(dialog).toHaveTextContent(entry.whyItMatters);
    expect(dialog).toHaveTextContent("What to try");
    expect(dialog).toHaveTextContent(entry.tip);
  });

  it("does not open on hover alone", async () => {
    const user = userEvent.setup();
    render(<ExplainPopover entry={entry} />);
    await user.hover(screen.getByRole("button", { name: /What is/ }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes on Escape and gives focus back to the trigger", async () => {
    const user = userEvent.setup();
    render(<ExplainPopover entry={entry} />);
    const trigger = screen.getByRole("button", { name: /What is/ });
    await user.click(trigger);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveAttribute("aria-expanded", "false"));
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("closes on a click outside", async () => {
    const user = userEvent.setup();
    render(
      <div>
        <p>Outside</p>
        <ExplainPopover entry={entry} />
      </div>,
    );
    const trigger = screen.getByRole("button", { name: /What is/ });
    await user.click(trigger);
    await screen.findByRole("dialog");
    await user.click(screen.getByText("Outside"));
    await waitFor(() => expect(trigger).toHaveAttribute("aria-expanded", "false"));
  });

  it("renders on the server without the content", () => {
    const html = renderToString(<ExplainPopover entry={entry} />);
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain(entry.tip);
  });
});

describe("Meaning", () => {
  it("is the one-sentence meaning in muted text", () => {
    render(<Meaning entry={entry} />);
    const p = screen.getByText(entry.meaning);
    expect(p.tagName).toBe("P");
    expect(p.className).toContain("text-ink-soft");
  });
});

describe("explained rows", () => {
  it("a weakness row shows the meaning under its name and a popover outside its link", () => {
    render(
      <WeaknessRow
        label="Not winning won games"
        state="active"
        rate={{ mean: 0.2, low: 0.15, high: 0.25 }}
        evidence={{ events: 8, chances: 40, sufficient: true, needMore: null }}
        band={{ median: 0.1 }}
        href="/app/weaknesses/W9a"
        trend={[]}
        onset={null}
        explain={entry}
      />,
    );
    expect(screen.getByText(entry.meaning)).toBeInTheDocument();
    const button = screen.getByRole("button", { name: /What is/ });
    expect(button.closest("a")).toBeNull();
  });

  it("a KPI chart shows the meaning under its heading with a popover", () => {
    render(
      <KpiTrendGrid
        items={[
          {
            metric: "conversion_rate",
            label: "Conversion",
            unit: "rate",
            points: [{ week: "2026-11-02", value: 0.5, low: 0.4, high: 0.6, n: 12 }],
            explain: entry,
          },
        ]}
        range={{ from: "2026-11-02", to: "2026-11-02" }}
        minN={5}
      />,
    );
    expect(screen.getByText(entry.meaning)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /What is/ })).toBeInTheDocument();
  });
});

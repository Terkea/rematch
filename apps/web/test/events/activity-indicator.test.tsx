// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { stubMatchMedia } from "../ui/helpers";

const { ActivityIndicator } = await import("../../src/components/activity-indicator.js");
const { queryKeys } = await import("../../src/lib/query-keys.js");
type Activity = import("../../src/lib/realtime/activity.js").Activity;

let qc: QueryClient;

beforeEach(() => {
  stubMatchMedia();
  qc = new QueryClient();
});

function show(activity?: Activity) {
  if (activity) qc.setQueryData(queryKeys.activity(), activity);
  return render(
    <QueryClientProvider client={qc}>
      <ActivityIndicator />
    </QueryClientProvider>,
  );
}

const at = Date.parse("2026-11-22T10:00:00Z");

describe("ActivityIndicator", () => {
  it("renders nothing when idle", () => {
    const { container } = show();
    expect(container).toBeEmptyDOMElement();
    const again = show({ imports: {}, failures: [] });
    expect(again.container).toBeEmptyDOMElement();
  });

  it("shows import progress against the known total", () => {
    show({ imports: { a1: { imported: 120, total: 400 } }, failures: [] });
    expect(screen.getByText("Importing 120 / 400 games")).toBeInTheDocument();
    expect(screen.queryByText(/live/i)).toBeNull();
  });

  it("counts what has arrived when the total is unknown", () => {
    show({ imports: { a1: { imported: 120 } }, failures: [] });
    expect(screen.getByText("Importing: 120 games so far")).toBeInTheDocument();
  });

  it("adds up several imports, and drops the total if one is unknown", async () => {
    show({
      imports: { a1: { imported: 100, total: 300 }, a2: { imported: 20, total: 100 } },
      failures: [],
    });
    expect(screen.getByText("Importing 120 / 400 games")).toBeInTheDocument();
    act(() =>
      qc.setQueryData(queryKeys.activity(), {
        imports: { a1: { imported: 100, total: 300 }, a2: { imported: 20 } },
        failures: [],
      }),
    );
    expect(await screen.findByText("Importing: 120 games so far")).toBeInTheDocument();
  });

  it("shows failures as a button that opens a list of them", async () => {
    const user = userEvent.setup();
    show({
      imports: {},
      failures: [
        { kind: "import-account", reason: "chess.com returned 410", at },
        { kind: "analysis", reason: "engine crashed", at },
        { kind: "analysis", reason: "timed out", at },
      ],
    });
    const button = screen.getByRole("button", { name: "3 jobs failed" });
    await user.click(button);
    expect(await screen.findByText("chess.com returned 410")).toBeInTheDocument();
    expect(screen.getByText("Games import")).toBeInTheDocument();
    expect(screen.getByText("timed out")).toBeInTheDocument();
  });

  it("says 1 job failed in the singular", () => {
    show({ imports: {}, failures: [{ kind: "analysis", reason: "x", at }] });
    expect(screen.getByRole("button", { name: "1 job failed" })).toBeInTheDocument();
  });

  it("clears the failures from the list", async () => {
    const user = userEvent.setup();
    const { container } = show({ imports: {}, failures: [{ kind: "analysis", reason: "x", at }] });
    await user.click(screen.getByRole("button", { name: "1 job failed" }));
    await user.click(await screen.findByRole("button", { name: "Clear" }));
    expect(qc.getQueryData<Activity>(queryKeys.activity())?.failures).toEqual([]);
    expect(container).toBeEmptyDOMElement();
  });
});

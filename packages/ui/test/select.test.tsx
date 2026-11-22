import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SegmentedControl, Select } from "../src/primitives/index.js";

afterEach(cleanup);

const timeControls = [
  { value: "all", label: "All time controls" },
  { value: "blitz", label: "Blitz" },
  { value: "rapid", label: "Rapid" },
];

const results = [
  { value: "all", label: "All" },
  { value: "win", label: "Wins" },
  { value: "draw", label: "Draws" },
  { value: "loss", label: "Losses" },
];

describe("Select", () => {
  it("labels its trigger with the visible label and shows the chosen option", () => {
    render(
      <Select label="Time control" value="all" options={timeControls} onValueChange={vi.fn()} />,
    );
    const trigger = screen.getByRole("combobox", { name: "Time control" });
    expect(trigger).toHaveTextContent("All time controls");
  });

  it("calls onValueChange with the option's value when one is chosen", async () => {
    const onValueChange = vi.fn();
    const user = userEvent.setup();
    render(
      <Select
        label="Time control"
        value="all"
        options={timeControls}
        onValueChange={onValueChange}
      />,
    );
    await user.click(screen.getByRole("combobox", { name: "Time control" }));
    await user.click(await screen.findByRole("option", { name: "Rapid" }));
    expect(onValueChange).toHaveBeenCalledWith("rapid");
  });

  it("submits its value under name", () => {
    const { container } = render(
      <form>
        <Select
          label="Time control"
          name="tc"
          value="rapid"
          options={timeControls}
          onValueChange={vi.fn()}
        />
      </form>,
    );
    expect(container.querySelector('input[name="tc"]')).toHaveValue("rapid");
  });
});

function Controlled({ onValueChange }: { onValueChange: (v: string) => void }) {
  const [value, setValue] = useState("all");
  return (
    <SegmentedControl
      label="Result"
      value={value}
      options={results}
      onValueChange={(v) => {
        setValue(v);
        onValueChange(v);
      }}
    />
  );
}

describe("SegmentedControl", () => {
  it("is a labelled radio group with one checked option", () => {
    render(
      <SegmentedControl label="Result" value="win" options={results} onValueChange={vi.fn()} />,
    );
    const group = screen.getByRole("radiogroup", { name: "Result" });
    expect(group).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Wins" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "All" })).toHaveAttribute("aria-checked", "false");
  });

  it("names every radio, and its form input, by the label and never the raw value (M2)", () => {
    const options = [
      {
        value: "0845062f-0000-4000-8000-000000000001:blitz",
        label: "Blitz on chess.com (356 games)",
      },
      {
        value: "0845062f-0000-4000-8000-000000000001:rapid",
        label: "Rapid on chess.com (159 games)",
      },
    ];
    const { container } = render(
      <SegmentedControl
        label="Games coached"
        value={options[0]?.value ?? ""}
        options={options}
        onValueChange={vi.fn()}
      />,
    );
    for (const o of options) {
      expect(screen.getByRole("radio", { name: o.label })).toBeInTheDocument();
    }
    // Base UI's hidden native input carries the value; tools that read it must get the label.
    const inputs = [...container.querySelectorAll<HTMLInputElement>("input[type=radio]")];
    expect(inputs).toHaveLength(2);
    for (const [i, input] of inputs.entries()) {
      expect(input.getAttribute("aria-label")).toBe(options[i]?.label);
    }
  });

  it("moves the selection with ArrowRight", async () => {
    const onValueChange = vi.fn();
    const user = userEvent.setup();
    render(<Controlled onValueChange={onValueChange} />);
    await user.tab();
    expect(screen.getByRole("radio", { name: "All" })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(onValueChange).toHaveBeenLastCalledWith("win");
    expect(screen.getByRole("radio", { name: "Wins" })).toHaveFocus();
    expect(screen.getByRole("radio", { name: "Wins" })).toHaveAttribute("aria-checked", "true");
  });

  it("has one tab stop (roving)", async () => {
    const user = userEvent.setup();
    render(
      <>
        <Controlled onValueChange={vi.fn()} />
        <button type="button">After</button>
      </>,
    );
    await user.tab();
    await user.tab();
    expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
  });
});

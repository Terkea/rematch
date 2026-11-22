import "@testing-library/jest-dom/vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  inputClasses,
  Select,
  Switch,
  Tabs,
  TabsList,
  TabsTrigger,
} from "../src/primitives/index.js";
import { tokensCss } from "./design-md.js";

// Task 9 critic: every state on the styleguide must read differently from Rest, and night
// floating surfaces must separate from the page (DESIGN.md "Interaction states").

afterEach(cleanup);

// Vitest runs from packages/ui (jsdom gives import.meta.url a non-file scheme).
const source = (path: string) => readFileSync(resolve(process.cwd(), "src", path), "utf8");
const classes = (el: Element | null) => (el?.getAttribute("class") ?? "").split(/\s+/);

describe("hover on controls that sit in a well", () => {
  it("steps a text field's edge to ink-soft, never on a disabled or invalid field", () => {
    expect(inputClasses.split(/\s+/)).toContain("enabled:not-aria-invalid:hover:border-ink-soft");
  });

  it("steps the select trigger's edge to ink-soft", () => {
    render(
      <Select
        label="Time control"
        value="blitz"
        options={[{ value: "blitz", label: "Blitz" }]}
        onValueChange={() => {}}
      />,
    );
    const trigger = document.querySelector("[data-slot='select-trigger']");
    expect(classes(trigger)).toContain("hover:border-ink-soft");
  });

  it("gives the switch a hover when off and when on, and none when disabled", () => {
    render(<Switch aria-label="Move sounds" />);
    const root = classes(screen.getByRole("switch", { name: "Move sounds" }));
    expect(root).toContain("not-data-disabled:hover:border-ink-soft");
    expect(root).toContain("not-data-disabled:data-checked:hover:bg-ink-soft");
  });
});

describe("disabled tabs", () => {
  it("drop to faint ink through Base UI's data-disabled", () => {
    render(
      <Tabs defaultValue="moves">
        <TabsList aria-label="Panels">
          <TabsTrigger value="moves">Moves</TabsTrigger>
          <TabsTrigger value="mistakes" disabled>
            Mistakes
          </TabsTrigger>
        </TabsList>
      </Tabs>,
    );
    const tab = screen.getByRole("tab", { name: "Mistakes" });
    expect(tab).toHaveAttribute("data-disabled");
    expect(classes(tab)).toContain("data-disabled:text-ink-faint");
  });
});

describe("night study", () => {
  it("steps hover up inside a sheet, since night well is near-black", () => {
    expect(tokensCss).toMatch(
      /\[data-theme="dark"\] \.bg-sheet,\s*\[data-theme="dark"\] \[data-surface="sheet"\]\s*\{[^}]*--surface-hover:\s*color-mix\(in srgb, var\(--color-sheet\), var\(--color-ink\) 14%\)/,
    );
  });

  it("dims the page to well behind a dialog and edges the dialog in rule-strong", () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Delete your account?</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    const popup = screen.getByRole("dialog");
    expect(classes(popup)).toContain("dark:border-rule-strong");
    const backdrop = document.querySelector("[data-slot='dialog-overlay']");
    expect(classes(backdrop)).toContain("dark:bg-well/90");
  });

  it("does the same for the drawer, the command menu, the popover and the select list", () => {
    for (const file of ["overlays/drawer.tsx", "overlays/command-menu.tsx"]) {
      expect(source(file)).toContain("dark:bg-well/90");
      expect(source(file)).toContain("dark:border-rule-strong");
    }
    for (const file of ["primitives/popover.tsx", "primitives/select.tsx"]) {
      expect(source(file)).toContain("dark:border-rule-strong");
    }
  });
});

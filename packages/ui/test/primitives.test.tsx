import "@testing-library/jest-dom/vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { GearSixIcon, Icon, TrashIcon } from "../src/icons/index.js";
import { Button, Field, Input, Switch } from "../src/primitives/index.js";

afterEach(cleanup);

describe("Button", () => {
  it("renders the primary variant as an ink fill with on-ink text", () => {
    render(<Button variant="primary">Review game</Button>);
    const button = screen.getByRole("button", { name: "Review game" });
    expect(button).toHaveClass("bg-ink", "text-on-ink", "rounded-field");
  });

  it("defaults to the primary variant", () => {
    render(<Button>Start drill</Button>);
    expect(screen.getByRole("button", { name: "Start drill" })).toHaveClass("bg-ink");
  });

  it("outlines the destructive variant in ink and never uses red (red marks mistakes)", () => {
    render(
      <Button variant="danger">
        <Icon icon={TrashIcon} />
        Delete account
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Delete account" });
    expect(button).toHaveClass("border-ink", "text-ink");
    // No pen-* token in any utility (text-pen-text, border-pen, hover:bg-pen/10, ...).
    const penClasses = button.className.split(/\s+/).filter((c) => /-pen(-|\/|$)/.test(c));
    expect(penClasses).toEqual([]);
  });

  it("renders its child element instead of a button with asChild", () => {
    render(
      <Button asChild variant="outline">
        <a href="/games">Open games</a>
      </Button>,
    );
    const link = screen.getByRole("link", { name: "Open games" });
    expect(link.tagName).toBe("A");
    expect(link).toHaveClass("border-rule-strong");
  });
});

describe("Field", () => {
  it("marks the control invalid and points aria-describedby at the error text", () => {
    render(
      <Field
        label="Email"
        hint="We send the sign-in link here."
        error="Enter a valid email address."
      >
        <Input type="email" />
      </Field>,
    );
    const input = screen.getByLabelText("Email");
    expect(input).toHaveAttribute("aria-invalid", "true");
    const error = screen.getByText("Enter a valid email address.");
    const describedBy = input.getAttribute("aria-describedby")?.split(" ") ?? [];
    expect(describedBy).toContain(error.id);
    expect(input).toHaveAccessibleDescription(/Enter a valid email address\./);
  });

  it("describes a valid control by its hint only", () => {
    render(
      <Field label="Username" hint="Your chess.com username.">
        <Input />
      </Field>,
    );
    const input = screen.getByLabelText("Username");
    expect(input).not.toHaveAttribute("aria-invalid");
    expect(input).toHaveAccessibleDescription("Your chess.com username.");
  });
});

describe("Icon", () => {
  it("is hidden from assistive tech without a label", () => {
    const { container } = render(<Icon icon={GearSixIcon} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(svg).toHaveAttribute("width", "20");
  });

  it("is a labelled image with a label", () => {
    render(<Icon icon={GearSixIcon} label="Settings" size={16} />);
    const img = screen.getByRole("img", { name: "Settings" });
    expect(img).toHaveAttribute("aria-label", "Settings");
    expect(img).not.toHaveAttribute("aria-hidden");
  });
});

describe("Switch", () => {
  it("toggles on Space", async () => {
    const user = userEvent.setup();
    render(<Switch aria-label="Board sounds" />);
    const toggle = screen.getByRole("switch", { name: "Board sounds" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await user.tab();
    expect(toggle).toHaveFocus();
    await user.keyboard(" ");
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.keyboard(" ");
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });
});

describe("Icon dependency owner", () => {
  // Vitest runs from packages/ui (jsdom gives import.meta.url a non-file scheme).
  const webSrc = resolve(process.cwd(), "../../apps/web/src");

  function sources(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return entry.name === "node_modules" ? [] : sources(path);
      return /\.(tsx?|jsx?|mjs)$/.test(entry.name) ? [path] : [];
    });
  }

  it("apps/web imports icons only from @ci/ui/icons, never Phosphor directly", () => {
    const files = sources(webSrc);
    expect(files.length).toBeGreaterThan(0);
    const offenders = files.filter((file) =>
      /from\s+["']@phosphor-icons\/|import\(\s*["']@phosphor-icons\/|require\(\s*["']@phosphor-icons\//.test(
        readFileSync(file, "utf8"),
      ),
    );
    expect(offenders).toEqual([]);
  });
});

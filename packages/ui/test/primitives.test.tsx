import "@testing-library/jest-dom/vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { GearSixIcon, Icon, TrashIcon } from "../src/icons/index.js";
import { overlay } from "../src/motion/index.js";
import {
  Button,
  Dialog,
  DialogContent,
  DialogTitle,
  Field,
  Input,
  Switch,
  Tabs,
  TabsList,
  TabsTrigger,
  touchTarget,
} from "../src/primitives/index.js";
import { code, designMd, tableAfter, tokensCss } from "./design-md.js";

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

describe("touch targets (44px on coarse pointers)", () => {
  const coarse = touchTarget.split(" ");

  it("expands the hit area only for coarse pointers, never the visual size", () => {
    expect(coarse.length).toBeGreaterThan(0);
    for (const c of coarse) expect(c.startsWith("pointer-coarse:")).toBe(true);
    expect(coarse).toContain("pointer-coarse:after:min-h-11");
    expect(coarse).toContain("pointer-coarse:after:min-w-11");
  });

  it("applies to both button sizes, asChild links and tabs", () => {
    render(
      <>
        <Button size="md">Review game</Button>
        <Button size="sm">Skip</Button>
        <Button asChild>
          <a href="/games">Open games</a>
        </Button>
        <Tabs defaultValue="moves">
          <TabsList>
            <TabsTrigger value="moves">Moves</TabsTrigger>
          </TabsList>
        </Tabs>
      </>,
    );
    const targets = [
      screen.getByRole("button", { name: "Review game" }),
      screen.getByRole("button", { name: "Skip" }),
      screen.getByRole("link", { name: "Open games" }),
      screen.getByRole("tab", { name: "Moves" }),
    ];
    for (const el of targets) expect(el).toHaveClass(...coarse);
  });

  it("applies to the dialog close", () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Disconnect chess.com?</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByRole("button", { name: "Close" })).toHaveClass(...coarse);
  });
});

describe("Tabs", () => {
  it("reserves the selected weight so labels never reflow", () => {
    render(
      <Tabs defaultValue="moves">
        <TabsList>
          <TabsTrigger value="moves">Moves</TabsTrigger>
          <TabsTrigger value="plans">Plans</TabsTrigger>
        </TabsList>
      </Tabs>,
    );
    for (const name of ["Moves", "Plans"]) {
      const tab = screen.getByRole("tab", { name });
      const ghost = tab.querySelector('[data-slot="tabs-label-reserve"]');
      expect(ghost).toHaveAttribute("aria-hidden", "true");
      expect(ghost).toHaveClass("invisible", "font-semibold");
      expect(ghost).toHaveTextContent(name);
    }
  });

  it("keeps the scrolling track off the pill, so focus rings aren't clipped at its rounded ends", () => {
    render(
      <Tabs defaultValue="moves">
        <TabsList>
          <TabsTrigger value="moves">Moves</TabsTrigger>
        </TabsList>
      </Tabs>,
    );
    const list = screen.getByRole("tablist");
    expect(list).not.toHaveClass("overflow-x-auto");
    expect(list.parentElement).toHaveClass("overflow-x-auto");
    expect(list.parentElement).not.toHaveClass("rounded-pill");
  });
});

describe("hover follows the tonal ladder", () => {
  it("uses the surface-hover step, not a fixed well", () => {
    render(
      <>
        <Button variant="outline">Find a game</Button>
        <Button variant="quiet">Skip</Button>
        <Button variant="danger">Delete account</Button>
      </>,
    );
    for (const name of ["Find a game", "Skip", "Delete account"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toHaveClass("hover:bg-(--surface-hover)");
      expect(button).not.toHaveClass("hover:bg-well");
    }
  });

  it("steps paper to sheet, and sheet to well, in tokens.css", () => {
    expect(tokensCss).toMatch(/:root\s*\{[^}]*--surface-hover:\s*var\(--color-sheet\)/);
    expect(tokensCss).toMatch(
      /\.bg-sheet,\s*\[data-surface="sheet"\]\s*\{[^}]*--surface-hover:\s*var\(--color-well\)/,
    );
  });
});

describe("Button asChild composition", () => {
  it("keeps the child's own ref and handlers, and forwards a non-button ref", () => {
    const childRef = createRef<HTMLAnchorElement>();
    const outerRef = createRef<HTMLElement>();
    const calls: string[] = [];
    render(
      <Button asChild ref={outerRef} onPointerDown={() => calls.push("button")}>
        <a
          href="/games"
          ref={childRef}
          onPointerDown={() => calls.push("child")}
          onKeyDown={() => calls.push("child-key")}
        >
          Open games
        </a>
      </Button>,
    );
    const link = screen.getByRole("link", { name: "Open games" });
    expect(childRef.current).toBe(link);
    expect(outerRef.current).toBe(link);
    fireEvent.pointerDown(link, { button: 0 });
    fireEvent.keyDown(link, { key: "Enter" });
    expect(calls).toEqual(["child", "button", "child-key"]);
  });

  it("merges onClick, className and style with the child's, as Radix Slot does", () => {
    const calls: string[] = [];
    render(
      <Button
        asChild
        variant="outline"
        className="w-full"
        style={{ color: "red", marginTop: 4 }}
        title="from button"
        onClick={() => calls.push("button")}
      >
        <a
          href="/games"
          className="underline"
          style={{ color: "blue" }}
          title="from child"
          onClick={(e) => {
            e.preventDefault();
            calls.push("child");
          }}
        >
          Open games
        </a>
      </Button>,
    );
    const link = screen.getByRole("link", { name: "Open games" });
    expect(link).toHaveClass("border-rule-strong", "w-full", "underline");
    expect(link.style.color).toBe("blue");
    expect(link.style.marginTop).toBe("4px");
    expect(link).toHaveAttribute("title", "from child");
    fireEvent.click(link);
    // The child prevented the default, so the Button's handler is skipped.
    expect(calls).toEqual(["child"]);
  });

  it("runs both onClick handlers, the child's first", () => {
    const calls: string[] = [];
    render(
      <Button asChild onClick={() => calls.push("button")}>
        <button type="button" onClick={() => calls.push("child")}>
          Review game
        </button>
      </Button>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Review game" }));
    expect(calls).toEqual(["child", "button"]);
  });

  it("always runs the first handler, even for an event that arrives already prevented", () => {
    const calls: string[] = [];
    const prevent = (e: Event) => e.preventDefault();
    window.addEventListener("click", prevent, { capture: true });
    try {
      render(
        <Button asChild onClick={() => calls.push("button")}>
          <a href="/games" onClick={() => calls.push("child")}>
            Open games
          </a>
        </Button>,
      );
      fireEvent.click(screen.getByRole("link", { name: "Open games" }));
    } finally {
      window.removeEventListener("click", prevent, { capture: true });
    }
    expect(calls).toEqual(["child"]);
  });
});

describe("Field error is announced", () => {
  it("sits in a polite live region", () => {
    render(
      <Field label="Email" error="Enter a valid email address.">
        <Input />
      </Field>,
    );
    const error = screen.getByText("Enter a valid email address.");
    expect(error.closest('[aria-live="polite"]')).not.toBeNull();
  });
});

describe("overlay values match DESIGN.md and tokens.css", () => {
  const rows = tableAfter("### Overlay values");
  const keys: Record<string, keyof typeof overlay> = {
    "--enter-dialog-y": "dialogY",
    "--enter-dialog-scale": "dialogScale",
    "--enter-float-scale": "floatScale",
    "--offset-float": "floatOffset",
  };

  it("has every overlay value and no others", () => {
    expect(rows.map(([token]) => code(token)).sort()).toEqual(Object.keys(keys).sort());
    expect(Object.keys(overlay).sort()).toEqual(Object.values(keys).sort());
  });

  for (const [token, key] of Object.entries(keys)) {
    it(`${token} agrees in DESIGN.md, tokens.css and @ci/ui/motion`, () => {
      const row = rows.find(([t]) => code(t) === token);
      const value = code(row?.[1]);
      expect(Number.parseFloat(value)).toBe(overlay[key]);
      expect(tokensCss).toContain(`${token}: ${value};`);
    });
  }

  it("DESIGN.md names them", () => {
    expect(designMd).toContain("### Overlay values");
  });
});

import "@testing-library/jest-dom/vitest";
import { GearSix } from "@phosphor-icons/react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { Icon } from "../src/icons/index.js";
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
    const { container } = render(<Icon icon={GearSix} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(svg).toHaveAttribute("width", "20");
  });

  it("is a labelled image with a label", () => {
    render(<Icon icon={GearSix} label="Settings" size={16} />);
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

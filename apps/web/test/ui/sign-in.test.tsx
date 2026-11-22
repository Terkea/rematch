// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const magicLink = vi.fn();
const social = vi.fn();
vi.mock("../../src/lib/auth-client.js", () => ({
  signIn: {
    magicLink: (...a: unknown[]) => magicLink(...a),
    social: (...a: unknown[]) => social(...a),
  },
}));

const { SignInForm } = await import("../../src/app/(auth)/sign-in/sign-in-form.js");
const { signInErrorMessage } = await import("../../src/app/(auth)/sign-in/messages.js");
const { BlockedNotice } = await import("../../src/app/(auth)/blocked/blocked-notice.js");
const { CheckEmail } = await import("../../src/app/(auth)/sign-in/check-email/check-email.js");

beforeEach(() => {
  magicLink.mockReset();
  social.mockReset();
});

describe("sign-in error messages", () => {
  it.each([
    ["link_expired", "This link has expired. Request a new one."],
    ["lichess_denied", "Lichess sign-in was cancelled. You can try again or use your email."],
    [
      "account_not_linked",
      "That email already has an account. Sign in with your email link, then connect Lichess from Accounts.",
    ],
    ["not_invited", "This email can't sign in yet. chess-improver is invite-only for now."],
  ])("%s", (code, message) => {
    expect(signInErrorMessage(code)).toBe(message);
  });

  it("has no message for an unknown or missing code", () => {
    expect(signInErrorMessage("nope")).toBeNull();
    expect(signInErrorMessage(undefined)).toBeNull();
  });
});

describe("SignInForm", () => {
  it("shows the email field and button, and Lichess and Google only when configured", () => {
    const { rerender } = render(<SignInForm google={false} lichess={false} />);
    expect(screen.getByLabelText("Email")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Email me a sign-in link" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Continue with Lichess" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Continue with Google" })).toBeNull();

    rerender(<SignInForm google lichess />);
    const buttons = screen.getAllByRole("button").map((b) => b.textContent);
    expect(buttons).toEqual([
      "Email me a sign-in link",
      "Continue with Lichess",
      "Continue with Google",
    ]);
  });

  it("shows the message for the error it was given", () => {
    render(<SignInForm google={false} lichess={false} error="link_expired" />);
    expect(screen.getByText("This link has expired. Request a new one.")).toBeInTheDocument();
  });

  it("requests a magic link and goes to check-email without the address in the URL", async () => {
    magicLink.mockResolvedValue({ data: { status: true }, error: null });
    const navigate = vi.fn();
    render(<SignInForm google={false} lichess={false} navigate={navigate} />);
    await userEvent.type(screen.getByLabelText("Email"), "dev@example.invalid");
    await userEvent.click(screen.getByRole("button", { name: "Email me a sign-in link" }));
    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/sign-in/check-email"));
    expect(magicLink).toHaveBeenCalledWith(
      expect.objectContaining({
        email: "dev@example.invalid",
        callbackURL: "/app/accounts",
        errorCallbackURL: "/sign-in",
      }),
    );
  });

  it("shows a pending state on the button while the request runs", async () => {
    magicLink.mockReturnValue(new Promise(() => {}));
    render(<SignInForm google={false} lichess={false} navigate={vi.fn()} />);
    await userEvent.type(screen.getByLabelText("Email"), "dev@example.invalid");
    await userEvent.click(screen.getByRole("button", { name: "Email me a sign-in link" }));
    const button = await screen.findByRole("button", { name: "Sending link" });
    expect(button).toHaveAttribute("aria-disabled", "true");
  });

  it("says when too many links were requested", async () => {
    magicLink.mockResolvedValue({ data: null, error: { status: 429 } });
    render(<SignInForm google={false} lichess={false} navigate={vi.fn()} />);
    await userEvent.type(screen.getByLabelText("Email"), "dev@example.invalid");
    await userEvent.click(screen.getByRole("button", { name: "Email me a sign-in link" }));
    expect(
      await screen.findByText("Too many sign-in links were requested. Try again in 10 minutes."),
    ).toBeInTheDocument();
  });

  it("passes errorCallbackURL /sign-in to Google", async () => {
    social.mockResolvedValue({ data: null, error: null });
    render(<SignInForm google lichess={false} />);
    await userEvent.click(screen.getByRole("button", { name: "Continue with Google" }));
    expect(social).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "google", errorCallbackURL: "/sign-in" }),
    );
  });
});

describe("BlockedNotice and CheckEmail", () => {
  it("says it once, as the one heading, with the exact copy", () => {
    render(<BlockedNotice />);
    const headings = screen.getAllByRole("heading");
    expect(headings).toHaveLength(1);
    expect(headings[0]).toHaveTextContent("You need to be 13 or older to use chess-improver.");
    expect(screen.getByText("We've deleted the details you gave us.")).toBeInTheDocument();
  });

  it("shows the address the link went to", () => {
    render(<CheckEmail email="dev@example.invalid" />);
    expect(screen.getByRole("heading", { name: "Check your email" })).toBeInTheDocument();
    expect(
      screen.getByText("We sent a sign-in link to dev@example.invalid. It expires in 10 minutes."),
    ).toBeInTheDocument();
  });

  it("falls back to a generic line when the address isn't known", () => {
    render(<CheckEmail email={null} />);
    expect(
      screen.getByText("We sent a sign-in link to your email. It expires in 10 minutes."),
    ).toBeInTheDocument();
  });
});

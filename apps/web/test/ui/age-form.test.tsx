// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgeForm } from "../../src/app/(auth)/welcome/age/age-form.js";
import { stubFetch } from "./helpers.js";

afterEach(() => vi.unstubAllGlobals());

const today = new Date("2026-11-21T12:00:00Z");

describe("AgeForm", () => {
  it("asks for month and year with the privacy hint", () => {
    render(<AgeForm today={today} navigate={vi.fn()} />);
    expect(screen.getByRole("group", { name: "When were you born?" })).toBeInTheDocument();
    expect(
      screen.getByText("We ask so we can keep younger players' accounts private by default."),
    ).toBeInTheDocument();
    const months = screen.getByLabelText("Month") as HTMLSelectElement;
    expect([...months.options].filter((o) => o.value).map((o) => o.textContent)).toHaveLength(12);
    expect(months.options[1]?.textContent).toBe("January");
    const years = screen.getByLabelText("Year") as HTMLSelectElement;
    const values = [...years.options].filter((o) => o.value).map((o) => Number(o.value));
    expect(values[0]).toBe(2026);
    expect(values.at(-1)).toBe(1900);
    expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });

  it("asks for both fields before sending", async () => {
    const calls = stubFetch([]);
    render(<AgeForm today={today} navigate={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Choose the month and year you were born.")).toBeInTheDocument();
    expect(calls).toHaveLength(0);
  });

  it("posts the month and year as JSON and follows the redirect", async () => {
    const calls = stubFetch([
      { method: "POST", path: "/api/age", body: { redirectTo: "/app/accounts" } },
    ]);
    const navigate = vi.fn();
    render(<AgeForm today={today} navigate={navigate} />);
    await userEvent.selectOptions(screen.getByLabelText("Month"), "6");
    await userEvent.selectOptions(screen.getByLabelText("Year"), "1990");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/app/accounts"));
    expect(calls[0]?.body).toEqual({ year: 1990, month: 6 });
    expect(calls[0]?.headers.get("content-type")).toBe("application/json");
  });

  it("goes to /blocked when the server says so", async () => {
    stubFetch([{ method: "POST", path: "/api/age", body: { redirectTo: "/blocked" } }]);
    const navigate = vi.fn();
    render(<AgeForm today={today} navigate={navigate} />);
    await userEvent.selectOptions(screen.getByLabelText("Month"), "1");
    await userEvent.selectOptions(screen.getByLabelText("Year"), "2015");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/blocked"));
  });

  it("explains a future month", async () => {
    stubFetch([
      { method: "POST", path: "/api/age", status: 400, body: { error: "invalid_birth_month" } },
    ]);
    render(<AgeForm today={today} navigate={vi.fn()} />);
    await userEvent.selectOptions(screen.getByLabelText("Month"), "12");
    await userEvent.selectOptions(screen.getByLabelText("Year"), "2026");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByText("That month hasn't happened yet. Choose your birth month."),
    ).toBeInTheDocument();
  });
});

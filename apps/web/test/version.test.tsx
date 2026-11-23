// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { brand } from "../src/brand.js";
import { appVersion, versionLabel } from "../src/version.js";

// One product version (ADR 0095): the root package.json is the source; release-please keeps
// apps/web/package.json in step, and the app shows it. Paths are from apps/web (vitest's cwd).
const rootVersion: string = JSON.parse(readFileSync(resolve("../../package.json"), "utf8")).version;

vi.mock("../src/lib/auth-client", () => ({ signOut: vi.fn() }));

describe("the app version", () => {
  it("is the root package version", () => {
    expect(appVersion).toBe(rootVersion);
  });

  it("says beta while the version is 0.y.z, and drops it from 1.0.0 (the MVP)", () => {
    expect(versionLabel("Rematch", "0.1.0")).toBe("Rematch v0.1.0 · beta");
    expect(versionLabel("Rematch", "0.12.3")).toBe("Rematch v0.12.3 · beta");
    expect(versionLabel("Rematch", "1.0.0")).toBe("Rematch v1.0.0");
  });

  it("shows in the account menu", async () => {
    const { UserMenu } = await import("../src/app/(app)/_shell/user-menu.js");
    render(<UserMenu email="adult@example.invalid" signOutTo={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Account menu" }));
    expect(await screen.findByText(versionLabel(brand.name, rootVersion))).toBeInTheDocument();
  });
});

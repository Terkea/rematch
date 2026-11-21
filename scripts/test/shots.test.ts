import { describe, expect, it, vi } from "vitest";
import { parseShotsArgs, shotMatrix, shotPath } from "../shots-lib.js";

describe("shotPath", () => {
  it("slugs nested routes", () => {
    expect(shotPath(".shots", "/lab/board/a", 390, "light")).toBe(
      ".shots/lab-board-a/390-light.png",
    );
  });
  it("names the root route home", () => {
    expect(shotPath(".shots", "/", 1440, "dark")).toBe(".shots/home/1440-dark.png");
  });
  it("appends a hash to the slug", () => {
    expect(shotPath(".shots", "/styleguide#hero", 390, "dark")).toBe(
      ".shots/styleguide--hero/390-dark.png",
    );
  });
});

describe("route safety", () => {
  it("sanitises characters that are illegal in file names", () => {
    expect(shotPath(".shots", '/a?x=1:*"<>|', 390, "light")).toBe(
      ".shots/a_x=1______/390-light.png",
    );
  });
});

describe("route normalisation", () => {
  it("prepends a slash when missing", () => {
    expect(parseShotsArgs(["--routes", "styleguide,lab"]).routes).toEqual(["/styleguide", "/lab"]);
  });
  it("keeps /a,/b and warns on drive-looking paths", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(parseShotsArgs(["--routes", "/a,/b"]).routes).toEqual(["/a", "/b"]);
    expect(warn).not.toHaveBeenCalled();
    parseShotsArgs(["--routes", "C:/Program Files/Git/styleguide"]);
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});

describe("parseShotsArgs", () => {
  it("defaults", () => {
    const a = parseShotsArgs([]);
    expect(a.routes).toEqual(["/styleguide", "/sign-in", "/lab"]);
    expect(a.out).toBe(".shots");
    expect(a.baseUrl).toBe("http://localhost:3100");
    expect(a.motion).toBe(false);
  });
  it("splits a comma list of routes", () => {
    expect(parseShotsArgs(["--routes", "/a,/b"]).routes).toEqual(["/a", "/b"]);
  });
  it("reads the other flags", () => {
    const a = parseShotsArgs([
      "--base-url",
      "http://x:1",
      "--out",
      "o",
      "--storage",
      "s.json",
      "--motion",
    ]);
    expect(a).toMatchObject({ baseUrl: "http://x:1", out: "o", storage: "s.json", motion: true });
  });
});

describe("shotMatrix", () => {
  it("gives 8 paths for 2 routes", () => {
    expect(shotMatrix(".shots", ["/a", "/b"])).toHaveLength(8);
  });
});

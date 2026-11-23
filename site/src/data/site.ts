// Facts the landing page and the docs share. Every claim here is checked against the code and the
// decision log (docs/decisions); the ADR numbers are the evidence.
import rootPackage from "../../../package.json";

export const REPO = "Terkea/rematch";
export const REPO_URL = `https://github.com/${REPO}`;
export const BLOB = `${REPO_URL}/blob/main`;
export const DISCUSSIONS_URL = `${REPO_URL}/discussions`;
export const GOOD_FIRST_ISSUES_URL = `${REPO_URL}/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22`;
export const HELP_WANTED_URL = `${REPO_URL}/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22`;

/** The product version: the root package.json once release-please owns it, else the first beta. */
export const VERSION =
  "version" in rootPackage && typeof rootPackage.version === "string"
    ? rootPackage.version
    : "0.1.0";

/** A path under the site's base (/rematch/), with a trailing slash for pages. */
export function href(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  return `${base}/${path.replace(/^\//, "")}`;
}

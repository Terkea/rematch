// The report reveal (DESIGN.md choreography 2, ADR 0073): it runs on the first view of a report
// version on this device. "Seen" lives in localStorage under `ci.report.seen` (a short list of
// `<reportId>:<version>`). Every access is wrapped: a private window or blocked storage just
// means the page shows its end state, never an error.

export const SEEN_KEY = "ci.report.seen";
/** How many report versions are remembered; older ones fall off. */
const KEEP = 20;

export const seenId = (reportId: string, version: number) => `${reportId}:${version}`;

function readSeen(storage: Pick<Storage, "getItem"> | null | undefined): string[] {
  try {
    const raw = storage?.getItem(SEEN_KEY);
    const list: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function hasSeen(storage: Pick<Storage, "getItem"> | null | undefined, id: string): boolean {
  return readSeen(storage).includes(id);
}

export function markSeen(
  storage: Pick<Storage, "getItem" | "setItem"> | null | undefined,
  id: string,
): void {
  try {
    const list = readSeen(storage).filter((x) => x !== id);
    list.push(id);
    storage?.setItem(SEEN_KEY, JSON.stringify(list.slice(-KEEP)));
  } catch {
    // Storage is unavailable or full: the reveal simply runs again next time.
  }
}

/**
 * The inline script that runs while the HTML is parsed, before the sections paint: on a first
 * view it sets `data-reveal="first"` on the report, so the CSS reveal in globals.css starts from
 * the server-rendered frame (no flash, no wait for hydration). Values are JSON-encoded with `<` escaped, so
 * they cannot break out of the string or the script.
 */
export function revealScript(elementId: string, id: string): string {
  const js = (v: string) => JSON.stringify(v).replace(/</g, "\\u003c");
  const key = js(SEEN_KEY);
  return `(function(){try{var el=document.getElementById(${js(elementId)});if(!el)return;var l=JSON.parse(localStorage.getItem(${key})||"[]");if(!Array.isArray(l)||l.indexOf(${js(id)})<0)el.setAttribute("data-reveal","first");}catch(e){}})();`;
}

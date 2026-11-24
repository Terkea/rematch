"""EXP-0019: draw the audit's sample and build the blind audit page (preregistration.md, section 7).

    python make_audit.py            # in the lab image, with the snapshot in the data directory

It reads the pinned Lichess puzzle snapshot (EXP-0007's external input), draws a seeded simple
random sample of the audience puzzles (the frame), tags it with the production Rust code through
`cli lab tag-motifs` (no engine), draws the audit's items from the frame's strata, and writes:

- `<data>/exp19/sealed/key.json`: the hidden key, every item's puzzle id, stratum, our tags and
  Lichess's themes. Kept out of git until the audit is done.
- `<data>/exp19/audit.html`: the page the person audits with. It holds the positions and lines
  only, in a seeded order, with no puzzle id, tag, theme or rating.
- `sample.csv` (this folder): the drawn puzzle ids, sorted, with nothing else.

It writes the same bytes every time: the draws come from `rematch_lab.experiments.seed`, and the
page and the key are serialised with sorted keys.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

import numpy as np
import polars as pl

import rematch_lab
from rematch_lab.experiments import Budget, external_input, seed
from rematch_lab.models import MotifLine

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import analysis as A  # noqa: E402  (the registered constants: tags, blocks, allocation)

EXP = A.EXP
SNAPSHOT = "lichess-puzzles/lichess_db_puzzle.csv"
SNAPSHOT_SHA256 = "8ce98180bcb7cb60f3f51f12afdbe6a4fd294d8aaf4f1410e8c25e85ac78fdc9"
PILOT = HERE.parent / "EXP-0007" / "sample-pilot.csv"
PIECES = HERE.parents[2] / "packages" / "ui" / "assets" / "pieces" / "cburnett"
OUT_DIR = "exp19"

# Section 7: EXP-0007's audience, the frame and the draws.
MIN_PLAYS, MIN_POPULARITY, MAX_RATING = 1000, 50, 2000
FRAME_SIZE = 20000
BATCH = 2000
# Practice items, from EXP-0007's pilot (development data, never analysed).
PRACTICE = ("AkJTy", "PwLEk", "wKuJU")
# The labelled items rotate: odd blocks take hangingPiece and sacrifice, even ones the others.
LABELLED_ROTATION = (("hangingPiece", "sacrifice"), ("backRankMate", "skewer"))
# The distractors alternate: odd blocks 2 with a theme and 1 without, even blocks 1 and 2.
DISTRACTOR_ROTATION = ((2, 1), (1, 2))
BUDGET = {"lines": FRAME_SIZE + len(PRACTICE), "cli_calls": FRAME_SIZE // BATCH + 1}


def load_audience(exclude: set[str]) -> tuple[pl.DataFrame, pl.DataFrame]:
    path = external_input(SNAPSHOT)
    if A.sha256_file(path) != SNAPSHOT_SHA256:
        raise SystemExit(f"{path} is not the registered snapshot")
    df = (
        pl.scan_csv(path, infer_schema_length=0)
        .select("PuzzleId", "FEN", "Moves", "Rating", "Popularity", "NbPlays", "Themes")
        .with_columns(pl.col("Rating", "Popularity", "NbPlays").cast(pl.Int64))
        .filter(
            ((pl.col("NbPlays") >= MIN_PLAYS) & (pl.col("Popularity") >= MIN_POPULARITY)
             & (pl.col("Rating") < MAX_RATING))
            | pl.col("PuzzleId").is_in(list(PRACTICE))
        )
        .collect()
        .sort("PuzzleId")
    )
    practice = df.filter(pl.col("PuzzleId").is_in(list(PRACTICE)))
    audience = df.filter(
        (pl.col("NbPlays") >= MIN_PLAYS)
        & (pl.col("Popularity") >= MIN_POPULARITY)
        & (pl.col("Rating") < MAX_RATING)
        & ~pl.col("PuzzleId").is_in(sorted(exclude))
    )
    return audience, practice


def tag(rows: list[tuple], budget: Budget) -> tuple[dict[str, dict], str]:
    lines = [MotifLine(id=pid, fen=fen, setup=mv.split(" ")[:1], pv=mv.split(" ")[1:])
             for pid, fen, mv in rows]
    out: dict[str, dict] = {}
    ids = set()
    for i in range(0, len(lines), BATCH):
        chunk = lines[i : i + BATCH]
        budget.spend("lines", len(chunk))
        budget.spend("cli_calls", 1)
        resp = rematch_lab.tag_motifs(chunk)
        ids.add(resp.algorithm_id)
        for line in resp.lines:
            out[line.id] = {
                "tags": None if line.best is None else sorted(line.best.tags),
                "mate_in": None if line.best is None else line.best.mate_in,
            }
    if len(ids) != 1:
        raise SystemExit(f"the tagging spans {len(ids)} algorithm ids")
    return out, ids.pop()


def stratum_of(tags: list[str], themes: set[str]) -> list[str]:
    fired = [t for t in A.TAGS if t in tags]
    if not fired:
        return ["D:miss" if themes.intersection(A.TAGS) else "D:none"]
    return [("L:" if t in themes else "U:") + t for t in fired]


def build() -> None:
    with PILOT.open(encoding="utf-8") as f:
        pilot = {line.split(",")[0] for line in f.read().splitlines()[1:] if line}
    audience, practice_df = load_audience(pilot)
    rng = np.random.default_rng(seed(EXP, "sample/frame"))
    mask = np.zeros(audience.height, dtype=bool)
    mask[rng.permutation(audience.height)[:FRAME_SIZE]] = True
    frame = audience.filter(pl.Series(mask))
    budget = Budget(BUDGET)
    rows = frame.select("PuzzleId", "FEN", "Moves").rows()
    rows += practice_df.select("PuzzleId", "FEN", "Moves").rows()
    tagged, algorithm_id = tag(rows, budget)
    info = {pid: {"fen": fen, "moves": mv.split(" "), "rating": r, "themes": th.split(" ")}
            for pid, fen, mv, r, th in
            pl.concat([frame, practice_df]).select("PuzzleId", "FEN", "Moves", "Rating",
                                                   "Themes").rows()}

    # The frame's strata (a puzzle we tag with two of the four is in two strata).
    strata: dict[str, list[str]] = defaultdict(list)
    excluded = []
    for pid in frame["PuzzleId"].to_list():
        tags = tagged[pid]["tags"]
        if tags is None:
            excluded.append(pid)
            continue
        for s in stratum_of(tags, set(info[pid]["themes"])):
            strata[s].append(pid)
    order_rng = np.random.default_rng(seed(EXP, "sample/items"))
    queues = {s: [ids[i] for i in order_rng.permutation(len(ids))]
              for s, ids in sorted(strata.items())}
    taken = {s: 0 for s in queues}

    def take(s: str, k: int) -> list[str]:
        got = queues[s][taken[s] : taken[s] + k]
        if len(got) < k:
            raise SystemExit(f"stratum {s} has only {len(queues[s])} puzzles")
        taken[s] += k
        return got

    shuffle = np.random.default_rng(seed(EXP, "sample/order"))
    items: list[dict[str, Any]] = []
    placed: dict[str, dict[str, Any]] = {}
    for block in range(1, A.BLOCKS + 1):
        drawn: list[tuple[str, str]] = []
        for t in A.TAGS:
            drawn += [(f"U:{t}", p) for p in take(f"U:{t}", A.PER_BLOCK[f"U:{t}"])]
        for t in LABELLED_ROTATION[(block - 1) % 2]:
            drawn += [(f"L:{t}", p) for p in take(f"L:{t}", 1)]
        n_miss, n_none = DISTRACTOR_ROTATION[(block - 1) % 2]
        drawn += [("D:miss", p) for p in take("D:miss", n_miss)]
        drawn += [("D:none", p) for p in take("D:none", n_none)]
        fresh = []
        for s, pid in drawn:
            if pid in placed:  # drawn for two strata: shown once, counted in both
                placed[pid]["strata"].append(s)
                continue
            placed[pid] = {"puzzle_id": pid, "block": block, "strata": [s]}
            fresh.append(placed[pid])
        for i in shuffle.permutation(len(fresh)):
            items.append(fresh[int(i)])
    for n, it in enumerate(items, start=1):
        pid = it["puzzle_id"]
        it.update({
            "item": n,
            "strata": sorted(it["strata"]),
            "tags": tagged[pid]["tags"],
            "themes": sorted(info[pid]["themes"]),
            "rating": info[pid]["rating"],
        })

    def shown(pid: str) -> dict[str, Any]:
        mv = info[pid]["moves"]
        return {"fen": info[pid]["fen"], "setup": mv[0], "line": mv[1:],
                "mate": tagged[pid]["mate_in"] is not None}

    page_items = [{"n": it["item"], "block": it["block"], **shown(it["puzzle_id"])}
                  for it in items]
    page_practice = [{"n": i + 1, **shown(p)} for i, p in enumerate(PRACTICE)]
    page_data = {"blocks": A.BLOCKS, "items": page_items, "practice": page_practice}
    page_id = hashlib.sha256(json.dumps(page_data, sort_keys=True).encode()).hexdigest()[:16]
    html = page(page_data, page_id)
    html_sha = hashlib.sha256(html.encode()).hexdigest()

    key = {
        "format": A.KEY_FORMAT,
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "snapshot_sha256": SNAPSHOT_SHA256,
        "page_id": page_id,
        "page_sha256": html_sha,
        "frame": {"size": FRAME_SIZE, "audience": audience.height, "excluded": sorted(excluded),
                  "strata": {s: len(v) for s, v in sorted(strata.items())}},
        "items": items,
        "practice": [{"n": i + 1, "puzzle_id": p, "tags": tagged[p]["tags"]}
                     for i, p in enumerate(PRACTICE)],
        "compute": budget.record(),
    }
    out = external_input(OUT_DIR)
    out.mkdir(parents=True, exist_ok=True)
    (out / "sealed").mkdir(exist_ok=True)
    (out / "sealed" / "key.json").write_text(json.dumps(key, indent=1, sort_keys=True) + "\n",
                                  encoding="utf-8")
    (out / "audit.html").write_text(html, encoding="utf-8")
    ids = sorted(it["puzzle_id"] for it in items)
    (HERE / "sample.csv").write_text("puzzle_id\n" + "".join(f"{i}\n" for i in ids),
                                     encoding="utf-8")
    print(json.dumps({"items": len(items), "page_id": page_id, "page_sha256": html_sha,
                      "key_sha256": A.sha256_file(out / "sealed" / "key.json"),
                      "frame": key["frame"]["strata"], "excluded": len(excluded),
                      "algorithm_id": algorithm_id}, indent=1))


# ---- the page --------------------------------------------------------------------------------


def piece_symbols() -> str:
    """The cburnett pieces (Colin M.L. Burnett, BSD 3-Clause, packages/ui/assets/pieces) as
    inline SVG symbols."""
    out = []
    for colour, c in (("l", "w"), ("d", "b")):
        for p in "kqrbnp":
            svg = (PIECES / f"Chess_{p}{colour}t45.svg").read_text(encoding="utf-8")
            inner = re.search(r"<svg[^>]*>(.*)</svg>", svg, re.S)
            assert inner, p
            body = re.sub(r"\s+", " ", inner.group(1)).strip()
            out.append(f'<symbol id="{c}{p.upper()}" viewBox="0 0 45 45">{body}</symbol>')
    return "".join(out)


DEFINITIONS = {
    "hangingPiece": (
        "Hanging piece",
        "Does the line win a knight, bishop, rook or queen that was left unguarded, so it is "
        "taken for nothing?",
        "Early in the line (usually its first move) a knight, bishop, rook or queen is captured "
        "that no other enemy piece was guarding, and the opponent cannot take back on that "
        "square. The point is a piece left loose and taken for free. A pawn does not count, and "
        "nor does a piece that was guarded (even too few times: winning it by attacking it more "
        "often than it is defended is another idea). It still counts when the capture also "
        "gives check or mate.",
    ),
    "skewer": (
        "Skewer",
        "Does a bishop, rook or queen attack a valuable piece, make it move off the line, and "
        "then take the piece behind it?",
        "A bishop, rook or queen attacks a valuable enemy piece along a line (often the king, "
        "with check). That piece moves out of the way, and the same attacker then captures the "
        "piece that stood behind it on the same line. It is the reverse of a pin: in a pin the "
        "piece in front is the less valuable one and cannot move. A piece that leaves the line "
        "for another reason, and is not attacked along it, does not make a skewer.",
    ),
    "sacrifice": (
        "Sacrifice",
        "Does the side to move give up material on purpose (two pawns' worth or more) and get "
        "more back, or mate, by the end of the line?",
        "The side to move deliberately gives up material it does not get straight back: at some "
        "point in the line it is at least two pawns' worth down (a knight or bishop for nothing, "
        "a rook for a minor piece, the queen for a rook). By the end of the line it has won more "
        "than it gave, at least two pawns' worth up overall, or it has given mate. A trade in "
        "which the material comes straight back is not a sacrifice.",
    ),
    "backRankMate": (
        "Back-rank mate",
        "Does the line end in mate by a rook or queen on the king's own back rank, the king "
        "boxed in by its own pieces in front of it?",
        "The line ends in checkmate by a rook or a queen on the mated king's back rank (rank 1 "
        "for White, rank 8 for Black), with the king standing on that rank. The king cannot "
        "step forward off it because every square in front of it holds one of its own pieces, "
        "usually its pawns. A mate on the back rank where an enemy piece, not the king's own, "
        "covers a square in front of it does not count.",
    ),
}


def page(data: dict[str, Any], page_id: str) -> str:
    questions = [{"tag": t, "name": DEFINITIONS[t][0], "q": DEFINITIONS[t][1],
                  "def": DEFINITIONS[t][2]} for t in A.TAGS]
    payload = json.dumps({**data, "page_id": page_id, "questions": questions,
                          "format": A.ANSWERS_FORMAT, "min_blocks": A.MIN_BLOCKS},
                         sort_keys=True, separators=(",", ":"))
    payload = payload.replace("<", "\\u003c")
    return (TEMPLATE.replace("__PIECES__", piece_symbols())
            .replace("__DATA__", payload))


TEMPLATE = r"""<!doctype html>
<html lang="en-GB">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'; form-action 'none'">
<title>Motif audit</title>
<style>
/* The Rematch tokens (packages/ui/DESIGN.md), copied because the page is standalone. The page
   loads no font, so the type falls back to the system's. */
:root {
  --color-paper: #F3F5F8; --color-sheet: #FBFCFD; --color-well: #E7EBF1;
  --color-rule: #C9D1DD; --color-rule-strong: #7A879E;
  --color-ink: #16233B; --color-ink-soft: #3D4A63; --color-ink-faint: #5B6780;
  --color-on-ink: #FBFCFD; --color-pencil: #56657F;
  --color-board-light: #E3E8EF; --color-board-dark: #A7B4C7;
  --color-board-lastmove: rgb(22 35 59 / 0.16);
  --color-selection: rgb(86 101 127 / 0.28);
  --text-xs: 0.75rem; --text-sm: 0.875rem; --text-base: 1rem; --text-lg: 1.125rem;
  --text-xl: 1.3125rem;
  --radius-board: 2px; --radius-field: 6px; --radius-sheet: 10px; --radius-pill: 9999px;
  --space-panel-pad: 16px; --space-gutter: 16px;
  --board-size: min(100vw - 2 * var(--space-gutter), 480px);
  color-scheme: light dark;
}
@media (prefers-color-scheme: dark) {
  :root {
    --color-paper: #121A2B; --color-sheet: #18223A; --color-well: #0D1421;
    --color-rule: #2C3956; --color-rule-strong: #6B7B9B;
    --color-ink: #E7ECF4; --color-ink-soft: #B4BFD2; --color-ink-faint: #8E9AB1;
    --color-on-ink: #121A2B; --color-pencil: #97A6C0;
    --color-board-light: #8796B5; --color-board-dark: #5A6A8C;
    --color-board-lastmove: rgb(231 236 244 / 0.30);
    --color-selection: rgb(151 166 192 / 0.34);
  }
}
* { box-sizing: border-box; }
::selection { background: var(--color-selection); }
body { margin: 0; background: var(--color-paper); color: var(--color-ink);
  font: var(--text-base)/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; }
header { display: flex; flex-wrap: wrap; gap: 8px 16px; align-items: baseline;
  padding: 12px var(--space-gutter); border-bottom: 1px solid var(--color-rule); }
header h1 { font-size: var(--text-lg); margin: 0; }
.meta { color: var(--color-ink-soft); font-size: var(--text-sm); }
.bar { flex: 1 1 160px; height: 4px; background: var(--color-well); border-radius: var(--radius-pill); overflow: hidden; align-self: center; }
.bar i { display: block; height: 100%; background: var(--color-ink); width: 0; }
main { display: grid; gap: var(--space-gutter); padding: var(--space-gutter);
  grid-template-columns: minmax(0, 1fr); max-width: 1180px; margin: 0 auto; }
@media (min-width: 900px) { main { grid-template-columns: auto minmax(0, 1fr); } }
.sheet { background: var(--color-sheet); border: 1px solid var(--color-rule);
  border-radius: var(--radius-sheet); padding: var(--space-panel-pad); }
#board { width: var(--board-size); height: var(--board-size); display: block; border-radius: var(--radius-board); }
.board-col { display: flex; flex-direction: column; gap: 10px; width: var(--board-size); }
.tomove { font-size: var(--text-sm); color: var(--color-ink-soft); }
.moves { display: flex; flex-wrap: wrap; gap: 4px; font-variant-numeric: tabular-nums; min-height: 2em; }
.moves span { padding: 1px 6px; border-radius: var(--radius-field); color: var(--color-ink-soft); }
.moves span.setup { color: var(--color-ink-faint); }
.moves span.now { background: var(--color-ink); color: var(--color-on-ink); }
.moves span.ahead { opacity: 0.45; }
.controls { display: flex; gap: 6px; flex-wrap: wrap; }
button { font: inherit; font-size: var(--text-sm); color: var(--color-ink); background: var(--color-well);
  border: 1px solid var(--color-rule-strong); border-radius: var(--radius-field); padding: 6px 12px; cursor: pointer; }
button:focus-visible, .q:focus-visible { outline: 2px solid var(--color-ink); outline-offset: 2px; }
button.primary { background: var(--color-ink); color: var(--color-on-ink); border-color: var(--color-ink); }
button[aria-pressed="true"] { background: var(--color-ink); color: var(--color-on-ink); border-color: var(--color-ink); }
button:disabled { opacity: 0.5; cursor: default; }
.qs { display: flex; flex-direction: column; gap: 10px; }
.q { border: 1px solid var(--color-rule); border-radius: var(--radius-field); padding: 10px 12px; }
.q.focus { border-color: var(--color-ink); }
.q h2 { font-size: var(--text-base); margin: 0 0 2px; }
.q p { margin: 0 0 8px; color: var(--color-ink-soft); font-size: var(--text-sm); }
.q .opts { display: flex; gap: 6px; }
details { font-size: var(--text-sm); color: var(--color-ink-soft); }
details p { margin: 6px 0; }
.nav { display: flex; gap: 8px; justify-content: space-between; align-items: center; flex-wrap: wrap; margin-top: 12px; }
.keys { font-size: var(--text-xs); color: var(--color-ink-faint); }
kbd { font: inherit; border: 1px solid var(--color-rule-strong); border-radius: 4px; padding: 0 4px; }
.notice { font-size: var(--text-sm); color: var(--color-ink-soft); }
.hidden { display: none !important; }
.coord { font-size: 2.2px; fill: var(--color-ink); }
</style>
</head>
<body>
<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>__PIECES__</defs></svg>
<header>
  <h1>Motif audit</h1>
  <span class="meta" id="progress"></span>
  <span class="bar" aria-hidden="true"><i id="bar"></i></span>
  <span class="meta" id="clock"></span>
  <button id="download">Download answers.json</button>
</header>
<main>
  <div class="board-col">
    <svg id="board" viewBox="0 0 80 80" role="img" aria-label="Board"></svg>
    <div class="tomove" id="tomove"></div>
    <div class="moves" id="moves" aria-live="polite"></div>
    <div class="controls">
      <button id="first" title="Start (Home)">|&lt;</button>
      <button id="prev" title="Back a move (Left)">&lt;</button>
      <button id="next" title="Forward a move (Right)">&gt;</button>
      <button id="last" title="End of the line (End)">&gt;|</button>
      <button id="flip" title="Flip the board (F)">Flip</button>
    </div>
  </div>
  <section class="sheet">
    <div id="intro" class="notice"></div>
    <div class="qs" id="qs"></div>
    <div class="nav">
      <button id="back">Previous item</button>
      <span class="keys"><kbd>Y</kbd> yes · <kbd>N</kbd> no · <kbd>U</kbd> unsure · <kbd>↑</kbd><kbd>↓</kbd> question · <kbd>←</kbd><kbd>→</kbd> moves · <kbd>Enter</kbd> next item</span>
      <button id="forward" class="primary">Next item</button>
    </div>
    <div id="blockdone" class="notice hidden"></div>
  </section>
</main>
<script>
"use strict";
const D = __DATA__;
const STORE = "rematch-exp19-" + D.page_id;
const ITEMS = D.practice.map(p => ({...p, practice: true})).concat(D.items);
const Q = D.questions;
let st = load();
let cur = st.cur || 0, ply = 1, focusQ = 0, flipped = false, shownAt = Date.now();

function load() {
  try { const s = JSON.parse(localStorage.getItem(STORE)); if (s && s.page_id === D.page_id) return s; } catch (e) {}
  return {page_id: D.page_id, answers: {}, practice: {}, seconds: {}, cur: 0};
}
function save() { try { localStorage.setItem(STORE, JSON.stringify(st)); } catch (e) {} }
function bucket(it) { return it.practice ? st.practice : st.answers; }

// ---- chess: FEN, UCI moves, attacks --------------------------------------------------------
function parseFen(fen) {
  const [placement, side, castling, ep] = fen.split(" ");
  const b = {};
  placement.split("/").forEach((row, r) => {
    let f = 0;
    for (const ch of row) {
      if (/\d/.test(ch)) f += +ch;
      else { b["abcdefgh"[f] + (8 - r)] = ch; f++; }
    }
  });
  return {b, side, castling, ep};
}
const colourOf = p => p === p.toUpperCase() ? "w" : "b";
function attacked(b, sq, by) {
  const f = sq.charCodeAt(0) - 97, r = +sq[1];
  const at = (df, dr) => { const nf = f + df, nr = r + dr; return nf >= 0 && nf < 8 && nr >= 1 && nr <= 8 ? b[String.fromCharCode(97 + nf) + nr] : undefined; };
  const is = (p, t) => p && colourOf(p) === by && p.toLowerCase() === t;
  const dir = by === "w" ? -1 : 1;
  if (is(at(-1, dir), "p") || is(at(1, dir), "p")) return true;
  for (const [df, dr] of [[1,2],[2,1],[-1,2],[-2,1],[1,-2],[2,-1],[-1,-2],[-2,-1]]) if (is(at(df, dr), "n")) return true;
  for (const [df, dr] of [[1,1],[1,0],[0,1],[-1,1],[-1,0],[0,-1],[1,-1],[-1,-1]]) if (is(at(df, dr), "k")) return true;
  for (const [df, dr, ts] of [[1,0,"rq"],[-1,0,"rq"],[0,1,"rq"],[0,-1,"rq"],[1,1,"bq"],[1,-1,"bq"],[-1,1,"bq"],[-1,-1,"bq"]]) {
    for (let k = 1; k < 8; k++) { const p = at(df * k, dr * k); if (p === undefined) { if (f + df * k < 0 || f + df * k > 7 || r + dr * k < 1 || r + dr * k > 8) break; continue; }
      if (colourOf(p) === by && ts.includes(p.toLowerCase())) return true; break; }
  }
  return false;
}
function apply(pos, uci) {
  const b = {...pos.b}, from = uci.slice(0, 2), to = uci.slice(2, 4), promo = uci[4];
  const p = b[from], cap = b[to];
  let captured = cap;
  delete b[from];
  if (p.toLowerCase() === "p" && from[0] !== to[0] && !cap) { const s = to[0] + from[1]; captured = b[s]; delete b[s]; }
  if (p.toLowerCase() === "k" && Math.abs(from.charCodeAt(0) - to.charCodeAt(0)) === 2) {
    const rank = from[1], kingside = to[0] === "g";
    const rf = (kingside ? "h" : "a") + rank, rt = (kingside ? "f" : "d") + rank;
    b[rt] = b[rf]; delete b[rf];
  }
  b[to] = promo ? (colourOf(p) === "w" ? promo.toUpperCase() : promo.toLowerCase()) : p;
  const side = pos.side === "w" ? "b" : "w";
  return {b, side, from, to, moved: p, captured};
}
function label(before, after, isLast, mate) {
  const p = after.moved, kind = p.toLowerCase();
  if (kind === "k" && Math.abs(after.from.charCodeAt(0) - after.to.charCodeAt(0)) === 2) return after.to[0] === "g" ? "O-O" : "O-O-O";
  let s = (kind === "p" ? "" : kind.toUpperCase()) + after.from + (after.captured ? "×" : "–") + after.to;
  if (kind === "p" && after.b[after.to].toLowerCase() !== "p") s += "=" + after.b[after.to].toUpperCase();
  const k = Object.keys(after.b).find(q => after.b[q] === (after.side === "w" ? "K" : "k"));
  if (isLast && mate) s += "#"; else if (k && attacked(after.b, k, colourOf(p))) s += "+";
  return s;
}
function positions(it) {
  const start = parseFen(it.fen);
  const out = [{...start, from: null, to: null}];
  let pos = start;
  const all = [it.setup].concat(it.line);
  all.forEach((m, i) => { const nx = apply(pos, m); nx.text = label(pos, nx, i === all.length - 1, it.mate); out.push(nx); pos = nx; });
  return out;
}

// ---- drawing -------------------------------------------------------------------------------
function drawBoard(pos, white) {
  const sq = 10; let s = "";
  for (let r = 0; r < 8; r++) for (let f = 0; f < 8; f++) {
    const file = white ? f : 7 - f, rank = white ? 8 - r : r + 1;
    const name = "abcdefgh"[file] + rank, light = (file + rank) % 2 === 1;
    s += `<rect x="${f*sq}" y="${r*sq}" width="${sq}" height="${sq}" fill="var(--color-board-${light ? "light" : "dark"})"/>`;
    if (name === pos.from || name === pos.to) s += `<rect x="${f*sq}" y="${r*sq}" width="${sq}" height="${sq}" fill="var(--color-board-lastmove)"/>`;
    const p = pos.b[name];
    if (p) s += `<use href="#${colourOf(p)}${p.toUpperCase()}" x="${f*sq}" y="${r*sq}" width="${sq}" height="${sq}"/>`;
    if (f === 0) s += `<text class="coord" x="${f*sq + 0.5}" y="${r*sq + 2.6}">${rank}</text>`;
    if (r === 7) s += `<text class="coord" x="${f*sq + sq - 2}" y="${r*sq + sq - 0.6}">${"abcdefgh"[file]}</text>`;
  }
  document.getElementById("board").innerHTML = s;
}

// ---- the item ------------------------------------------------------------------------------
let POS = [];
function render() {
  const it = ITEMS[cur];
  POS = positions(it);
  const mover = POS[1].side; // the side to move after the setup move
  const white = (mover === "w") !== flipped;
  drawBoard(POS[ply], white);
  document.getElementById("tomove").textContent =
    (mover === "w" ? "White" : "Black") + " to move, after the opponent's last move (shaded). The line below is the solution, both sides' moves.";
  const mv = document.getElementById("moves"); mv.innerHTML = "";
  POS.slice(1).forEach((p, i) => {
    const el = document.createElement("span"); el.textContent = p.text;
    if (i === 0) el.className = "setup";
    if (i + 1 === ply) el.classList.add("now"); else if (i + 1 > ply) el.classList.add("ahead");
    el.onclick = () => { ply = i + 1; render(); };
    mv.appendChild(el);
  });
  const total = D.items.length, done = Object.keys(st.answers).length;
  document.getElementById("progress").textContent = it.practice
    ? `Practice ${it.n} of ${D.practice.length} (not counted)`
    : `${it.n} of ${total} · block ${it.block} of ${D.blocks}`;
  document.getElementById("bar").style.width = (100 * done / total) + "%";
  document.getElementById("intro").textContent = it.practice
    ? "Practice: three puzzles to learn the page. Their answers are not counted."
    : "";
  const qs = document.getElementById("qs"); qs.innerHTML = "";
  const a = bucket(it)[it.n] || {};
  Q.forEach((q, i) => {
    const box = document.createElement("div"); box.className = "q" + (i === focusQ ? " focus" : ""); box.tabIndex = -1;
    box.innerHTML = `<h2>${i + 1}. ${q.name}</h2><p>${q.q}</p>`;
    const opts = document.createElement("div"); opts.className = "opts";
    for (const v of ["yes", "no", "unsure"]) {
      const bt = document.createElement("button"); bt.textContent = v[0].toUpperCase() + v.slice(1);
      bt.setAttribute("aria-pressed", a[q.tag] === v ? "true" : "false");
      bt.onclick = () => answer(i, v); opts.appendChild(bt);
    }
    box.appendChild(opts);
    const det = document.createElement("details"); det.innerHTML = `<summary>Definition</summary><p>${q.def}</p>`;
    box.appendChild(det); qs.appendChild(box);
  });
  document.getElementById("back").disabled = cur === 0;
  document.getElementById("forward").textContent = cur === ITEMS.length - 1 ? "Finish" : "Next item";
  tick();
}
function answer(i, v) {
  const it = ITEMS[cur], bk = bucket(it);
  bk[it.n] = {...(bk[it.n] || {}), [Q[i].tag]: v};
  focusQ = Math.min(i + 1, Q.length - 1); save(); render();
}
function leave() {
  const it = ITEMS[cur];
  if (!it.practice) st.seconds[it.n] = Math.round(((st.seconds[it.n] || 0) + (Date.now() - shownAt) / 1000) * 10) / 10;
  shownAt = Date.now();
}
function go(to) {
  if (to < 0 || to >= ITEMS.length) return;
  const it = ITEMS[cur];
  leave(); cur = to; st.cur = cur; ply = 1; focusQ = 0; save(); render();
  const nx = ITEMS[cur], bd = document.getElementById("blockdone");
  if (!it.practice && !nx.practice && nx.block !== it.block && to > 0) {
    const mins = Math.round(Object.values(st.seconds).reduce((x, y) => x + y, 0) / 60);
    bd.textContent = `Block ${it.block} of ${D.blocks} done, about ${mins} minutes of judging so far. You may stop after any whole block once you are past 90 minutes (at least ${D.min_blocks} blocks). Download answers.json whenever you stop.`;
    bd.classList.remove("hidden");
  } else bd.classList.add("hidden");
}
function forward() {
  const it = ITEMS[cur], a = bucket(it)[it.n] || {};
  const missing = Q.filter(q => !a[q.tag]);
  if (missing.length && !confirm(`${missing.length} question(s) unanswered; they will count as unsure. Go on?`)) return;
  if (cur === ITEMS.length - 1) { leave(); save(); download(); return; }
  go(cur + 1);
}
function tick() {
  const mins = Math.round(Object.values(st.seconds).reduce((x, y) => x + y, 0) / 60);
  document.getElementById("clock").textContent = `${mins} min`;
}
function download() {
  leave(); save();
  const out = {format: D.format, page_id: D.page_id, answers: st.answers, practice: st.practice, seconds: st.seconds,
               items: D.items.length, saved_at: new Date().toISOString()};
  const blob = new Blob([JSON.stringify(out, null, 1)], {type: "application/json"});
  const a = document.createElement("a"); a.href = URL.createObjectURL(blob); a.download = "answers.json";
  document.body.appendChild(a); a.click(); a.remove();
}
document.getElementById("first").onclick = () => { ply = 1; render(); };
document.getElementById("prev").onclick = () => { ply = Math.max(0, ply - 1); render(); };
document.getElementById("next").onclick = () => { ply = Math.min(POS.length - 1, ply + 1); render(); };
document.getElementById("last").onclick = () => { ply = POS.length - 1; render(); };
document.getElementById("flip").onclick = () => { flipped = !flipped; render(); };
document.getElementById("back").onclick = () => go(cur - 1);
document.getElementById("forward").onclick = forward;
document.getElementById("download").onclick = download;
document.addEventListener("keydown", e => {
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  const k = e.key;
  if (k === "ArrowRight") { ply = Math.min(POS.length - 1, ply + 1); render(); }
  else if (k === "ArrowLeft") { ply = Math.max(0, ply - 1); render(); }
  else if (k === "Home") { ply = 1; render(); }
  else if (k === "End") { ply = POS.length - 1; render(); }
  else if (k === "ArrowDown") { focusQ = Math.min(Q.length - 1, focusQ + 1); render(); }
  else if (k === "ArrowUp") { focusQ = Math.max(0, focusQ - 1); render(); }
  else if (k === "y" || k === "Y") answer(focusQ, "yes");
  else if (k === "n" || k === "N") answer(focusQ, "no");
  else if (k === "u" || k === "U") answer(focusQ, "unsure");
  else if (k >= "1" && k <= "4") { focusQ = +k - 1; render(); }
  else if (k === "f" || k === "F") { flipped = !flipped; render(); }
  else if (k === "Enter") forward();
  else if (k === "Backspace") go(cur - 1);
  else return;
  e.preventDefault();
});
setInterval(tick, 30000);
render();
</script>
</body>
</html>
"""


if __name__ == "__main__":
    build()

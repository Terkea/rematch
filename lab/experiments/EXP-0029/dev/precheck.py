import json, sys, statistics
def load(m, f):
    out = {}
    with open(f"{m}/players.jsonl", encoding="utf-8") as fh:
        for line in fh:
            p = json.loads(line); h = p["h"]
            if f(h, p): out[h] = p
    return out
frac = lambda h: int(h[:16], 16) / 2**64
jul = load("2026-07", lambda h, p: p["rapid_games"] >= 20 and frac(h) < 0.3)
aug = load("2026-08", lambda h, p: h in jul and p["eval_games"] >= 10)
sep = load("2026-09", lambda h, p: h in aug and p["eval_games"] >= 10)
oct_ = load("2026-10", lambda h, p: h in sep)
g = sorted(p["rapid_games"] for p in oct_.values() if p["rapid_games"] >= 5)
q = lambda x: g[int(x * (len(g) - 1))]
print(json.dumps({"jul": len(jul), "aug_sep": len(sep), "oct_ge5": len(g),
  "median_oct_rapid": statistics.median(g), "p25": q(.25), "p75": q(.75),
  "share_ge40": round(sum(x >= 40 for x in g) / len(g), 3),
  "share_ge25": round(sum(x >= 25 for x in g) / len(g), 3)}))

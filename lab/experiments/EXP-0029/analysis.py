"""EXP-0029: the stage 1 placeholder of the registered analysis.

Stage 1 registers the design (preregistration.md) before any new month is read. The analysis of
section 8, the sizing step of section 6 and their tests are written in stage 2, from EXP-0009's
`analysis.py`, reviewed by a fresh critic, and replace this file before any game is observed;
section 10 then states their sha256.

Its external inputs (the cohort, `inputs.sha256`) are not on any machine yet, so the lab check
does not run it. Run by hand, it refuses.
"""

from __future__ import annotations

import sys


def main() -> int:
    print("EXP-0029: stage 1 registers the design only; the analysis is written in stage 2",
          file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())

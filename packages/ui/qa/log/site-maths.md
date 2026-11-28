# Site maths pages (ADR 0118)

## 2026-11-28, round 1

- Shots: a fixture `math.md` (site/test/fixtures/lab/experiments/EXP-9001/math.md) built as `/docs/experiments/exp-0030/math/`, at 1440 and 390, day and night, served by `astro preview` and taken with Playwright. No horizontal page scroll at 390 (scrollWidth 375 = clientWidth).
- Critic: a fresh subagent with the shots, DESIGN.md and the critique prompt only.
- Verdict: **pass**, no blocker or major. Inline maths on the body baseline; display maths centred with even spacing; formulas take `ink` in both themes; nothing overflows at 390.
- Minor, not changed:
  1. The proof's end mark sits at the start of the next paragraph: that is the fixture's text, not the styling; the template puts it at the end of the proof's line.
  2. The Notation table's code chips wrap at 390: the docs' existing table and code styles, not the maths.
  3. An inline fraction loosens its line: KaTeX's struts; text-style fractions are left as they are.
  4. Display maths centred in a left-hung column: maths convention, kept.
  5. "Result 1." in the sans bold: Markdown bold, kept.

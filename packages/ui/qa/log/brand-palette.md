# Brand palette and hand-made touches (feat/brand-palette)

Critic: one fresh subagent that saw only the screenshots, DESIGN.md, REFERENCES.md and the critique prompt. Shots: `.shots/brand/{hero,brand,foundations,stats,charts,sign-in}-{390,1440}-{light,dark}.png` and `.shots/styleguide/{390,1440}-{light,dark}.png` (fixtures only).

## Round 1: revise

- major (B2): night `well` `#141210` read as black slabs on the warm sheet.
- major (B1): night pen `#F0705F` read as coral, near terracotta.
- minor: Caveat at `2xl` looked timid; the crayon read as a thin underline; the night board looked dim and grey; day last-move tint looked grey; sign-in copy is template-neutral (copy lane).

Changed: night `well` to `#191612` (`piece-dark` stays `#141210`), night pen to `#EE6258` (OKLCH hue 27°), notes to Caveat 600 at `3xl`, the crayon to two passes (5px and 2.5px), the night board warmed to `#A39580` / `#716552`. Contrast table re-run: 0 failures.

## Round 2: revise

- major: the crayon stopped after "you". Cause: `pathLength` dashes are measured on screen under `non-scaling-stroke`. Changed: the stroke is always fully drawn and revealed by a left-to-right clip.
- minor: the note wrapped at 390. Changed: `2xl` below 640px, `3xl` from 640px.

## Round 3: pass

Two minors left: the night eval-graph plot and empty-state wells are the darkest masses (warm, acceptable), and the sign-in copy belongs to the coach-voice lane.

## Amendment 1: back to the cool scoresheet (2026-11-23)

Shots: `/styleguide`, `/sign-in` and the site landing at 390 and 1440, day and night, from fixtures (scratchpad `holistic-2/brand-shots/blue/`), plus `og.png` and both README lockups. A fresh critic saw only the shots, DESIGN.md and REFERENCES.md.

- It found no warm, cream or brown tones: a pixel scan found warm hues only in the crayon swatches and strokes. Paper, sheet and well are cool in day, night study is navy, the boards are blue-grey, and the rasters and lockups use navy ink and pencil only. The crayon shows only under "there you go" (`B19`, `B20` respected).
- major: on the night landing at 1440, the film's poster is the day still. It isn't a palette leftover: `site/public/media/rematch-hook.jpg` is one cool day frame of the film and has no night version, and this lane changed neither file. It belongs with the video and the site lanes.
- minor: the night "Watch the film" pill sits on that day poster (the same cause).
- minor: lazy feature images were blank in some full-page captures. That is the shots tool, not colour.
- nit: the night lockup is pale ink on transparent, for GitHub's dark `<picture>` source only.

Verdict: fail on one major, the film poster, which comes from before this lane and is outside its colour-token scope. Nothing in this lane's colour work needs fixing.

# 0102. The coach's voice: a patient voice at the board

Status: accepted, 2026-11-23. It is part of the Rematch brand (`docs/superpowers/specs/2026-11-23-rematch-brand-design.md`).

## Decision

The coach speaks in one shared voice, defined once in `backend/crates/ai/prompts/tone/voice` (v1) and included by the report, chat and weekly-review prompt sets. The voice belongs to someone sitting next to you at the board:
- short, plain sentences;
- it names the move and what it left undefended;
- it asks you to have another go at the position;
- it praises effort and progress only with a cited number;
- it never flatters, lectures or uses pet names;
- it never plays a role, so it never mentions being a parent.

The app's own coach-voice copy (trainer feedback, the practise card, the focus card) uses the same voice.

## Why

Rematch is built around a child learning from his dad. The numbers already tell the truth; the voice is how that truth is told, warmly enough that a player comes back tomorrow. One shared file keeps the report, chat and weekly review from drifting into three different voices.

## Constraints kept

- The validator is unchanged. Warmth never buys an uncited fact: "always", "never" and "every time" still need a number.
- Stored reports keep the prompt version and hash they were written with. Including the voice changes the hashes of the report, chat and weekly sets, and every older prompt file is kept. A DB test proves a stored report keeps its old hash and sections.
- The validator rejects "one" as a number word and, in weekly texts, "again" as a change word. So the prompt asks for "Let's have another go at this position." App copy is not validated and keeps "Let's try this one again."
- British spelling in the coach's output.

## Not in scope

The maintainer's dad story never appears in prompts or app copy (spec, anti-goals).

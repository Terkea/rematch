# 0041. First target: a private beta for the owner and a few friends

Date: 2026-11-21. Status: accepted (owner direction).

## Context

The owner: "dw about deployments and launch for now, i want to test it for myself so far ... or a few close friends". The roadmap had counted public deployment and launch hardening (about 70 unplanned tasks) towards "done".

## Decision

- **The target is a private beta:** the owner plus a few invited friends, using it for real on their own chess.com games.
- **Deferred:**
  - public production deployment and marketing pages;
  - launch hardening beyond what the beta needs;
  - commercial chess.com permission (personal use of the public API is fine for now).
- **Kept in full:**
  - import and analysis;
  - the profile engine;
  - AI reports and the coach chat;
  - the premium web UI;
  - drills (planned next).
- **Hosting:** the stack runs on the owner's machine through Docker Compose. Friends reach it later through a private tunnel or a small host, chosen when the owner wants it. Any exposure still needs the owner's go-ahead.
- **Access:** invite-only accounts with no public sign-up. Usage caps stay as safety limits.
- **AI auth (ADR 0062):**
  - Owner-only use may run on the owner's Claude Code token.
  - Once friends use it, Claude calls go through an `ANTHROPIC_API_KEY`, because Anthropic does not allow offering subscription access to other users.

## Consequences

- Priority order: the shortest path to the owner analysing their own games, then the profile, then AI reports and chat, then friend invites.
- Launch plans are not written until the owner asks.

## Sequence after the beta

The owner: "once we get the feedback from tests we can think about fixing the feedback then deployment launch and what not". The order is:
1. the beta;
2. collect feedback;
3. fix what the feedback shows;
4. only then plan deployment and launch.

## Amendment (2026-11-21): drills are deferred

Owner direction: "defer the drills for the beta". The interactive drills and training feature (the planned M5 drills plan) is out of the beta. The AI coaching report may still name the drill types it recommends as plain text. There is no drill trainer, puzzle player or spaced repetition in the beta. As with launch, the drills plan is written only once the owner asks after the beta feedback.

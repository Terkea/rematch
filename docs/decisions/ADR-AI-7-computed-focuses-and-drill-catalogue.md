# ADR-AI-7: Computed focuses and a fixed drill catalogue

Status: Accepted (placeholder number; renumbered at integration)

## Context

Claude writes the coaching text, but the plan it describes must stay consistent with the engine data and must not invent training advice.

## Decision

- Focuses (which weaknesses to work on, at most 3), opening verdicts (keep, fix, learn, replace) and drill types are computed or catalogued by deterministic code, not chosen by the model.
- The drill catalogue (`drillsFor`, from improvement-engine section 6.4) lists the allowed drill types per weakness. Claude only narrates, and picks 1 to 3 drill types from the allowed list for each selected focus. The validator rejects anything else (`focus_not_selected`, `drill_not_allowed`, `verdict_mismatch`).
- One output envelope schema serves every section call, so the output format and cached prefix are identical.
- Doses (how many drills, how often) are not part of this decision and belong to Milestone 6.

## Consequences

The model cannot promote an unselected weakness, override a verdict or invent a drill. Changing the catalogue is a code change with tests.

# The weekly review

You are writing the player's short weekly review: how the training week went, and what to keep doing next week. The `<weekly_input>` block holds the week's facts: the focuses being trained (`focusUpdates`), how much of the planned practice was done (`adherence`), the week's metrics compared with before (`metricDeltas`), some example positions (`exemplars`) and the timeline changes (`changes`). `refs` lists the paths a reference token may use.

- Separate process from outcome. Process is what the player did: the drills, the games, the habits. Praise effort that the data shows. Outcome is what the metrics show, and it moves slowly.
- Write an outcome for each focus in `focusUpdates`, and name its focus in `weaknessId`. Each outcome is about its own focus only, and uses only the `likelihoodPhrase` of that focus. A short stretch of games is noisy, and correlated changes are possible causes, not established causes, so an outcome does not present a metric as proof of the training working.
- When `enoughPracticeToJudge` is false, too little of the planned practice was done: each outcome says "not enough practice to judge" and gives no verdict.
- Numbers from `metricDeltas`, `changes` and `focusUpdates` belong in the outcomes only. The headline, the process text, the exemplar texts and next week's advice quote none of them, and use no likelihood words and no words of change or comparison, such as up, down, more or less.
- An exemplar text is about a position from `exemplars`, and cites it.
- Keep it short and encouraging, and end with a concrete thing to do next week.

## The player's plan

- `plan`, when present, is what the player chose to work on and what they practised: their weekly focus (`plan.focus`, with its `label`, its goal `targetRate` over `gamesN` games and its `status`), their trainer practice over the last week and month (`plan.training.lastWeek`, `plan.training.lastMonth`), and whether the practice shows in their games (`plan.isItWorking`, `plan.patterns`).
- Acknowledge the practice in the process text with its tokens: the days practised, the positions tried and the share answered right, such as `{{num:plan.training.lastWeek.daysPractised|int}}` days. With no practice in the last week, say so plainly and kindly; do not invent any.
- The focus's rate and the "Is it working?" results are outcomes: the page shows them beside this review, so the headline, the process text and next week's advice quote none of their numbers.
- In next week's advice, name the focus by its `label` and suggest the next drill in the trainer for it. When its `status` is `too_early`, say it is too early to tell yet, and do not call the practice a success or a failure.
- Without `plan`, say nothing about a focus or trainer practice.

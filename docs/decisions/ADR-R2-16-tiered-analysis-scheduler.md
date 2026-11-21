# ADR-R2-16. Tiered analysis scheduler

Status: accepted (selection rules, Task 26). Task 29 appends the job, `imported` status and preview sections.

## Selection rules (pure, `scheduler::select`)

Values are TS M2 Task 23 verbatim: free 1000/50/0/no P2/20 per week, pro 10 000/200/100/P2/300 per week; 15 per month, 36 months, 20 recent, 90 days, 10 plies, batch 25.

- **Primary time class.** The class with the most games in the last 90 days (ties to the longer class). A dormant account (nothing in 90 days) uses the last 365 days, then all time. P0 then takes the newest games regardless of date, and a dormant account has only its primary as active. Future-dated games (`played_at > now`) are ignored and logged.
- **P3 is rated only** (every new rated game created after the first scheduling run, within the weekly budget).
- **P0 before P3.** A new game goes to P0 while P0 has room (a free user with 30 analysed games sends new games to P0 until 50). This is the plan's literal behaviour, pinned by a test.
- **P2.** Calendar months are taken in UTC, up to and including 36 months back. The 15-per-month quota counts every analysed game of that month, whatever its tier, plus this run's picks. Rated games are taken first and spaced chronologically; casual games only fill what rated cannot. Picks are emitted newest first.
- **P2 for the primary time class runs in the same pass as P0** (P0 takes the newest 200, P2 up to 25 older). Kept on purpose; pinned by a test.
- **Equal `played_at`** is broken by game id.
- Candidates are `pending` with no tier until Task 3's `imported` status lands.

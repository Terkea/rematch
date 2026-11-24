# Competitors and Users: Research for Chess Improver

Date: 2026-11-21
Scope: competitor teardown, unmet needs of improving players, retention UX, differentiators, growth loops. Written against `docs/superpowers/specs/2026-11-21-chess-improver-design.md`.

## Method and caveats

- Sources are web searches and page fetches run on 2026-11-21: vendor sites, App Store listings, chess.com and Lichess forums and blogs, Product Hunt, Substack reviews, arXiv, and Duolingo's own blog.
- **Reddit was barely reachable.** The search engine returned almost no reddit.com pages, even for `site:reddit.com` queries. Chess.com and Lichess forum threads say the same things, so they stand in for Reddit here. One Reddit item did surface: Chessable's response to an r/chess pricing backlash in January 2025.
- **YouTube review summaries were not retrievable.** No review transcripts or summaries came back in the searches.
- Much of the evidence is anecdotal: forum posts, single reviews, and vendor marketing. Prices change often. Figures marked *(unverified)* come from third-party pages or old posts, so check them before using them in pricing decisions.

---

## 1. Competitor teardown

### Aimchess (owned by chess.com through Play Magnus Group)
- **Core features:** Imports games from chess.com, Lichess and Chess24. Scores the player in six areas (Tactics, Endgame, Advantage Capitalization, Resourcefulness, Time Management, Opening Performance) and compares them with players at a similar rating. The Training Room has Advantage Capitalization (replay games you lost from winning positions), Retry Mistakes, Opening Improver, Intuition Trainer, Blunder Preventer (pick the non-blunder of two moves), Checkmate Patterns, Blindfold Tactics and a 360 Trainer. A **Daily Plan** of about a dozen puzzles. **Scouting**: a 40-game report on an opponent. [chess.com review](https://www.chess.com/blog/SheldonOfOsaka/aimchess-a-review), [itechguides](https://www.itechguides.com/products/aimchess/)
- **Pricing:** Freemium. The free tier allows 15 lessons per day. Old App Store figures are $9.99/month and $71.99/year; one reviewer pays about $15/month *(unverified for 2026)*. [App Store](https://apps.apple.com/us/app/aimchess-learn-chess-online/id1524941307?uo=4)
- **Onboarding:** Link an account, get the six-area dashboard, and do a daily plan.
- **Praised:** The six-area dashboard and peer comparison ("like having an AI coach", [Lichess blog](https://lichess.org/@/MatthewKCanada/blog/one-year-in/anSXOB77)). The Daily Plan adds structure and covers endgames people skip. Puzzles feel more realistic than puzzle-rush tactics.
- **Complaints:**
  - Retry Mistakes surfaces old, irrelevant mistakes.
  - Grading is harsh: "treats nearly every move except the best as a blunder". [forum](https://www.chess.com/forum/view/general/app-for-drilling-your-past-blunders?lc=1)
  - Bugs, login problems and subscriptions that are hard to cancel. App ratings have slid to about 3.4–3.5 *(aggregator)*. [marlvel](https://marlvel.ai/apps/aimchess-learn-chess-online/reviews), [forum](https://www.chess.com/forum/view/help-support/aimchess-3)
  - chess.com members ask why they should pay a second company for reports on their own games. [forum](https://www.chess.com/forum/view/site-feedback/aimchess-shouldnt-that-sort-of-feedback-be-available-to-members-already)
  - Little natural-language explanation, and no visible proof that training transfers to games.

### chess.com Game Review, Coach and Insights
- **Core features:**
  - Game Review: classified moves, "Brilliant" and "Great" badges, Coach text explanations, and Retry.
  - Insights (Diamond only): accuracy over time and by move number, results by opponent rating, a calendar by time of day and weekday, and Opening Insights for the top 10 openings. [support](https://support.chess.com/en/articles/8708925-what-is-insights-on-chess-com), [opening insights](https://www.chess.com/news/view/announcing-opening-insights)
- **Pricing:** Free accounts get 1 full review per day. Platinum was removed. Gold is $60/year and Diamond $144/year, per a community post *(unverified)*. [blog](https://www.chess.com/blog/Pipercody/the-new-prices-and-memberships), [prices changing](https://www.chess.com/article/view/prices-changing)
- **Praised:** It is fun and emotionally rewarding, and "Brilliant" works as a badge people share. [forum](https://www.chess.com/forum/view/game-analysis/share-your-brilliant-move-2) It is zero-effort because it is built in.
- **Complaints:**
  - **Generic Coach text** ("engine line with generic words slapped over it"). A concrete tactic was explained as a "development" principle. A move that hung a bishop was called a "SOLID choice". [roast](https://www.chess.com/blog/Elite-Reports/how-i-roasted-chess-coms-game-review), [forum](https://chess.com/forum/view/general/coach-review-worse-than-useless-can-harm-your-game)
  - **Labels that look arbitrary**, such as an "inaccuracy" for a 0.1 eval difference.
  - **Missing explanations** for some moves. [forum](https://www.chess.com/forum/view/game-analysis/why-does-the-coach-occasionally-just-not-explain-why-a-move-was-evaluated-the-way-it-was)
  - The mobile redesign lost the overall picture: no persistent eval graph, a slow horizontal move list, and suggested lessons removed. [critique](https://www.chess.com/forum/view/site-feedback/detailed-critique-of-new-game-review-110227953)
  - Insights is called "basically useless" by some, and others say rivals give it free. [forum](https://www.chess.com/forum/view/general/is-insights-worth-going-platinum)
  - All of it is **per game**. No cross-game diagnosis turns into a plan.

### Lichess Insights and "Learn from your mistakes"
- **Core features:** A free "answer engine" that lets you pick a metric (accuracy, centipawn loss, time pressure and more) against a dimension (phase, opening family or variation, piece moved and more). It needs server analysis of your games. "Learn from your mistakes" works one game at a time. Puzzles from your own games can be found through the player filter. [blog](https://lichess.org/blog/VmZbaigAABACtXQC/chess-insights), [LFYM](https://lichess.org/blog/WFvLpiQAACMA8e9D/learn-from-your-mistakes)
- **Pricing:** Free.
- **Praised:** Powerful and free.
- **Complaints:**
  - "A bit overwhelming". Users ask for a guide.
  - "Nothing really valuable… other than checking which phase". [forum](https://lichess.org/forum/general-chess-discussion/how-best-to-use-chess-insights)
  - Users want LFYM across many games, not one. [forum](https://lichess.org/forum/lichess-feedback/love-learn-from-your-mistakes-can-it-be-implemented-to-use-all-my-games)
- **Lesson for us:** raw query tools do not answer "so what do I do?".

### Chessable
- **Core features:** Courses by titled authors, trained with MoveTrainer spaced repetition. Chessable 3.0 added **Puzzle Connect**: personalised puzzles from your chess.com mistakes, 10 for Basic and up to 1,000 for PRO. **Priority Lines** focus study on the lines people actually play. Folder review is PRO-only. [Chessable 3.0](https://www.chess.com/news/view/announcing-chessable-3)
- **Pricing:** PRO is about $11.99/month or $74.99/year. Courses are bought separately, and PRO takes 20% off them. [App Store](https://apps.apple.com/app/id1523279049), [PRO changes](https://chess.com/blog/Chessable/new-year-big-changes-to-chessable-pro)
- **Praised:** MoveTrainer is "slick". The content quality is high.
- **Complaints:**
  - **Review backlog snowballs**: "over 9000 pending reviews", "unmanageable", "burdensome even after changing settings". [Lichess blog](https://lichess.org/@/zig_zag_zug_zwang/blog/a-data-driven-approach-to-chess-improvement-part-1-srs--woodpecker-method/i2k8ZqAn), [forum](https://lichess.org/ublog/OEPayaq2/discuss), [forum](https://www.chess.com/forum/view/chess-openings/learning-with-movetrainer-vs-a-video-series)
  - The mobile app reviews a whole course at once ("all or nothing"). [App Store](https://apps.apple.com/us/app/id1523279049)
  - "You'll spend hundreds" on courses. A pricing backlash led to a January 2025 r/chess response. [forum](https://www.chess.com/forum/view/general/opinions-about-chessable), [r/chess mirror](https://pol1.lr.ggtyler.dev/r/chess/comments/1hxl45y/chessable_responds_to_critical_feedback/m6aopnu/?context=3)
  - Memorised lines fail when tested in another context. [forum](https://lichess.org/ublog/OEPayaq2/discuss)

### Chessbook
- **Core features:**
  - A repertoire builder scoped by **rating range and coverage target**. Replies are ranked by popularity at your level, with peer results, master stats and engine evals.
  - Spaced repetition, plus "**Go to your biggest gap**".
  - A game check that compares your chess.com and Lichess games against your repertoire.
  - Mistake quizzes, and "Guess the Move" model games from your lines. [64squares review](https://64squares.substack.com/p/chessbook-review), [interview](https://saychess.substack.com/p/an-interview-with-marcus-buffett)
- **Pricing:** Free up to about 400 moves. Pro is $7.99/month or $79.99/year. [App Store](https://apps.apple.com/us/app/chessbook/id6466343415)
- **Praised:** A modern, fast UI that works well on mobile. Steady feature releases. More confidence and clock saved in the opening.
- **Complaints:**
  - Adding too much at once makes daily reviews overwhelming.
  - Needs an internet connection.
  - Confusing numbers.
  - No "how often do I make this mistake" aggregate.
  - "Not for beginners or intermediates". [appshunter](https://appshunter.io/ios/app/6466343415/reviews/2)

### Chessigma
- **Core features:**
  - Free, unlimited, client-side Stockfish 17 game review, with no account needed.
  - Training tools: Woodpecker, Blunder Shield, Conversion Trainer, Sparring and an Opening Trainer.
  - An AI "Supercoach" chat.
  - **Chess Wrapped**: a yearly story with a "chess personality" matched to a famous player, framed as a shareable identity card. [site](https://chessigma.com/), [Wrapped](https://www.chessigma.com/blog/chess-wrapped-2025)
- **Pricing:** Free core features, with paid plans for extras.
- **Takeaway:** it **resets the price anchor for single-game review to zero** and is already running a viral Wrapped loop. No bulk analysis yet (by its own FAQ). [FAQ](https://www.chessigma.com/blog/free-chess-game-analysis)

### Noctie
- **Core features:**
  - A human-like bot across 25 levels (about 400–2800), with human-like openings, mistakes and **move timing**.
  - Grades on every move.
  - Opening practice against the most common human replies.
  - Spaced-repetition puzzles from your own mistakes after each game.
  - 100+ lessons.
  - Import from chess.com and Lichess. [noctie.ai](https://noctie.ai)
- **Pricing:** €14/month or €96/year, with a 7-day trial. It claims more than 100k learners.
- **Praised:** The realistic sparring partner "adapts to my level". [Product Hunt](https://producthunt.com/products/noctie-ai/reviews)
- **Complaints:** "How do you learn without move explanations?" Users want full-sentence reasons. Analysis "lacks depth vs a human coach". [forum](https://www.chess.com/forum/view/chess-equipment/noctie-ai-how-do-you-learn-without-move-explanations)

### DecodeChess
- **Core features:** Natural-language explanations of engine moves (threats, plans, an "Idea-Problem-Solution" frame). Works from PGN or FEN. [site](https://decodechess.com/?p=3595)
- **Pricing:** Free tier of 1 game per day. Paid is about $8.25/month. [stork](https://www.stork.ai/en/decodechess)
- **Complaints:** Explanations "do not accumulate" from game to game. There is no profile or plan. [Substack](https://aicentral.substack.com/p/experience-beats-analysis)

### ChessTempo
- **Core features:** A huge tactics database with custom sets (spaced repetition possible). A free opening trainer with spaced repetition and PGN import. Endgame training. [ChessTempo](https://chesstempo.com/mobile), [AlternativeTo](https://alternativeto.net/software/chess-tempo/about)
- **Pricing:** Free with registration, plus paid premium.
- **Praised:** "Best tool I've found for memorizing openings". Serious, deep tooling.
- **Complaints:** A dated UX. Custom sets must be built on the website. Not connected to your own games.

### Listudy
- **Core features:** Free, open-source (AGPL) spaced repetition for openings, endgames and tactics. Upload a PGN or import a Lichess study. [listudy.org](https://listudy.org/), [SRS blog](https://listudy.org/en/blog/spaced-repetition-for-chess)
- **Limitation:** By its own admission it only helps with memorising, not understanding.

### Dr. Wolf (chess.com)
- **Core features:** A beginner-focused coach that plays with you and explains step by step. Four coach personas with voices. [chess.com](https://www.chess.com/play/apps/drwolf), [acquisition](https://chess.com/news/view/dr-wolf-joins-chess-com)
- **Pricing:** 3 free coaching games and a 1-week trial, then a subscription.
- **Praised:** Practice-based rather than video. Great for beginners.
- **Complaints:** Advice not "refined or flexible". Weak engine and opening variety. [App Store](https://apps.apple.com/us/app/id1353041020)

### Chessly (GothamChess)
- **Core features:** Creator-led courses, with a membership that unlocks 50+ courses. [chessly.com](https://chessly.com)
- **Pricing:** Courses about $60 each, plus a Plus membership *(unverified)*.
- **Reception:** Mixed (Trustpilot about 3.5).
- **Takeaway:** **creator distribution beats product features for reach.** [Trustpilot](https://ca.trustpilot.com/review/chessly.com)

### New AI coaches (2025–2026)
- **Chessiro Coach:** an interactive coach inside game review that pulls games from chess.com and Lichess. [blog](https://www.chess.com/blog/TastyWinter/we-built-an-ai-that-explains-chess-positions)
- **Grandmaster AI Agent:** a demo that finds the top 3 turning points, reads explanations aloud with text-to-speech, and highlights squares. [blog](https://www.chess.com/blog/grandmaster-ai-agent/introducing-grandmaster-ai-agent)
- **Maia-3 / maiachess.com:** a free, open-source human-like engine with a combined Maia and Stockfish analysis view. [announcement](https://lichess.org/@/ashtonanderson/blog/announcing-maiachesscom/DWhVq7m0), [Maia background](https://news.cornell.edu/stories/2021/01/chess-engine-sacrifices-mastery-mimic-human-play)
- **Hobby LLM coaches:** chesscoach.dev, LLM-ChessCoach (GPT-5-Nano with Stockfish), Arrakis Engine. [nightfox](https://lichess.org/@/nightfox/blog/ai-chess-coach-20/EAtFkK61), [GitHub](https://github.com/ai-chess-training/LLM-ChessCoach)
- **Puzzles from your own games:** Chessie, CheckMyMate, "Learn from your blunders" and Chessis. A crowded but shallow niche. [Product Hunt](https://www.producthunt.com/p/chessie/chessie-2)
- **Evidence for our core rule:** a 2026 paper found GPT-5.4 commentary without tools had **22% incorrect sub-claims**, and open models exceeded 40%. Engine tools "substantially improve" correctness but **do not fully fix** it, and coverage of expert ideas stays limited. [arXiv 2608.04240](https://arxiv.org/abs/2608.04240), [Lichess blog](https://lichess.org/@/A8086555/blog/why-cant-llms-play-chess-well/3IzNykuo)

### Price map (per month, annual price where known)

| Product | Monthly | Annual |
|---|---|---|
| Noctie | €14 | €96 |
| Chessable PRO | $11.99 | $74.99 |
| Aimchess | ~$9.99 *(old)* | ~$71.99 *(old)* |
| DecodeChess | ~$8.25 | n/a |
| Chessbook | $7.99 | $79.99 |
| chess.com Diamond | n/a | ~$144 *(unverified)* |
| Chessigma review | free | free |
| Lichess | free | free |

Rematch is free. The cross-game diagnosis has to beat Chessigma and Lichess on something visible, and single-game review is not where it can.

---

## 2. Top unmet needs and recurring complaints (ranked by how often they came up)

1. **"I don't know what to study."** It is a direction problem, not a chess problem: "openings they never reach, endgames they don't lose". Players want "a clear study plan instead of just engine analysis". [forum](https://www.chess.com/forum/view/general/how-do-you-decide-what-to-study-after-analyzing-your-games)
2. **Generic or wrong explanations.** chess.com Coach text is generic or contradicts the engine. LLM hallucination is measured at 22% or more without tools.
3. **Puzzles don't transfer to my games.** "Generic daily puzzles… rating barely moved because the puzzles didn't target my own mistakes". Puzzles stop helping after about 2000 puzzle rating. [blog](https://www.chess.com/blog/TFUEL-1534/why-i-was-stuck-at-1200-and-what-actually-helped-me-improve), [forum](https://www.chess.com/forum/view/general/does-solving-puzzles-actually-help-your-rating-feeling-stuck)
4. **Mistake drills across many games, recent, and graded leniently.** Lichess LFYM covers one game at a time. Aimchess serves stale mistakes and grades harshly. Players want "just make a move better than your blunder". [forum](https://www.chess.com/forum/view/general/app-for-drilling-your-past-blunders?lc=1)
5. **Review backlog and overwhelm.** Chessable reaches 9000+ due reviews. Chessbook's reviews "get overwhelming". Lichess Insights is "overwhelming".
6. **Analysis is per game and doesn't accumulate.** DecodeChess "doesn't build over time". chess.com review is game by game. Players want "how often do I make this mistake" (Chessbook review).
7. **Progress isn't visible or provable.** Most tools show a rating, accuracy and a streak. None shows that training reduced a specific error in real games. *(Synthesis: none of the products above shows "transfer".)*
8. **Explanations of *why*.** Noctie users ask for sentences. DecodeChess's appeal is the "why".
9. **The big picture in the review UI.** A persistent eval graph, a clickable move list, and played and best moves side by side. [critique](https://www.chess.com/forum/view/site-feedback/detailed-critique-of-new-game-review-110227953)
10. **Neglected areas.** "My brain knows I need endgames, but my heart prefers openings". Players need a nudge to train what they avoid.
11. **Billing trust.** Hard cancellations, and app-store purchases that don't match the advertised tier (Aimchess, chess.com). Rematch has nothing to bill.
12. **Mobile and offline drilling** (Chessbook, Chessable).

---

## 3. Retention UX patterns

**Patterns that work, with evidence:**
- **Streaks with slack.** Duolingo users at a 7-day streak are 2.4× more likely to return the next day. **Splitting the streak from the daily goal raised D14 retention by 3.3%.** Allowing more streak freezes raised daily active users by 0.38%. A **lower bar to keep the streak** kept 40% more people on a 7-day streak. [Duolingo](https://blog.duolingo.com/how-duolingo-streak-builds-habit), [Duolingo](https://blog.duolingo.com/improving-the-streak), [Mobbin 859 streaks](https://mobbin.com/blog/streaks-are-about-losing-a-study-of-859-streak-designs), [Lenny's](https://www.lennysnewsletter.com/p/behind-the-product-duolingo-streaks)
- **A fixed, finite daily plan.** The Aimchess Daily Plan (about 12 items) is praised for giving structure.
- **Spaced repetition**, but only with load control (Chessable shows what happens without it).
- **Badge moments.** chess.com "Brilliant" gets shared.
- **Human-feeling sparring.** Noctie's main source of praise.

**Patterns that annoy chess players:**
- **Streaks that punish.** A user called the streak "highly annoying… discouraging me from learning". Users report streaks lost to time zones or resets. Restores are refused after 48 hours. Lost long streaks make people want to quit. [forum](https://www.chess.com/forum/view/community/how-do-i-turn-off-the-streak), [support](https://support.chess.com/en/articles/9714718-what-are-streaks), [UX critique](https://uxdesign.cc/3-reframing-streaks-on-duolingo-5-ideas-for-a-more-healthy-and-flexible-approach-to-language-8fd89545771e)
- **Unbounded review queues** (Chessable and Chessbook).
- **Over-harsh grading**, where anything but the top engine move is wrong.
- **Paywalled basics** after an "upgrade" (Chessable PRO-only courses, chess.com tier shuffles).
- **Cosmetic praise** that contradicts reality ("SOLID choice" on a hung bishop).

---

## 4. Differentiators: ranked list for "the best app ever"

Each item is marked as either in the design already (strengthen it) or missing (add it).

1. **A cross-game diagnosis with a cost attached** (in design: weakness ranking by rating points). No competitor says "this habit cost you about 38 rating points in your last 200 blitz games". Make the cost the headline of every weakness card.
2. **A transfer proof loop** (in design: §12a). Make it the hero of the Progress page and the weekly email: "Forks missed per game: 0.81 → 0.42 since you started fork drills (n=60, clear improvement)". No competitor does this. It is our "it works" proof.
3. **A grounded coach that never lies** (in design). Market it explicitly. The 22% hallucination figure is the foil. Add a visible "show the evidence" affordance on every sentence.
4. **A bounded daily session** (missing detail). The design has a "daily training queue" but no cap. Add a 10, 20 or 40 minute session the player chooses. FSRS load is capped, so overflow is deferred and **never** shown as a debt counter. The queue mixes items from the climb plan's 3 active weaknesses. There is a "vacation / catch-up" mode.
5. **Lenient, win%-based drill grading** (missing). Accept any move within a win% tolerance of best (for example, ≤3–5% win% lost, or still winning by a clear margin). Only "the one only-move" puzzles need exact answers. Show "good enough, the best was X" rather than "wrong".
6. **Recency-weighted own-mistake drills** (partly in design). Default to the last 30 days, and let the player filter by time control, phase, motif or opening. Retire a puzzle once it has been solved cold a few times (an FSRS state).
7. **Human-likelihood (Maia) layer** (missing). Tag each mistake as "typical at your rating" or "unusual". Weight opening prep toward the replies humans at your band actually play. Suggest "practical" moves rather than engine-only moves. Maia is the research-backed way to say "this is what's holding 1400s back". *(Check Maia's weights and code licence before shipping. Run it server-side like Stockfish.)*
8. **Instant post-game micro-review** (missing). When a new game syncs, push the single critical moment (eval swing plus a 1-sentence grounded explanation) as an optional 30-second drill. This ties our habit loop to the user's *playing* habit, which is the strongest existing behaviour, rather than asking for a separate study habit.
9. **"Why" playback** (partly in design). For each key moment, *play out* the refutation PV on the board with red-pen arrows, not just text. It answers the Noctie and DecodeChess demand for explanations while staying engine-grounded.
10. **"Biggest gap" and priority-line repertoire building** (partly in design). Borrow from Chessbook and Chessable: the trainer starts from **lines the player actually reached** and opponent replies weighted by frequency at their band. Then add a one-tap "go to your biggest gap".
11. **Conversion and defence sparring vs a human-like bot** (in design as "beat the engine"). Use a Maia-level opponent, not full Stockfish, so conversion drills feel like real games at the player's level (Noctie's main source of praise).
12. **Ask-the-coach chat over your own data** (missing). Chessigma Supercoach and Chessiro have chat, but it is ungrounded. Ours answers "why do I lose with the London?" only through tools over the profile, opening tree and game DB, using the same claim validator. Cap it for fair use.
13. **A "Neglected area" nudge** (missing). If a ranked weakness sits in an area the player never trains (often endgames), surface it gently once a week.
14. **A big-picture review UI** (in design: game explorer). Always-visible eval graph, a two-column clickable move list, and played and best moves shown together. The chess.com redesign showed these are table stakes.
15. **Offline-capable PWA drills** (missing). Cache the day's queue and the FSRS state, and sync later.
16. **Coach and club seats** (missing). A coach dashboard over several students' reports and drills. Coaches are a trusted distribution channel and a higher price point.
17. **Pre-tournament opponent scouting** (Aimchess has it; missing). Opening prep from an opponent's *public* games, for OTB and arena prep only. Never during a game, and never with live hooks.

---

## 5. Viral and growth loops

**Evidence:**
- **Wrapped-style recaps** are now standard. chess.com has had a personal Year in Review since 2023. Chessigma runs "Chess Wrapped" with a personality card matched to famous players, explicitly for sharing. Rosen Score and other unofficial "Wrapped" sites exist because demand exists. [chess.com 2023](https://www.chess.com/blog/vitualis/explore-your-2023-year-in-review-new-feature-and-insights), [Chessigma](https://www.chessigma.com/blog/chess-wrapped-2025), [Rosen](https://www.chess.com/blog/TheAuthorOfChickens/rosen-score-and-chess-wrapped-2025)
- **Badges people share.** chess.com "Brilliant" moves have whole forum threads of sharing. [forum](https://www.chess.com/forum/view/game-analysis/share-your-brilliant-move-2)
- **Creator-led platforms** (Chessly via GothamChess, Chessable authors) get reach that tools don't. Chess.com's streamer programme links Twitch accounts and notifies viewers when they play a streamer. [chess.com streamers](https://chess.com/news/view/announcing-community-streamers-chesscom) Existing Twitch chess extensions are thin (profile widgets, spoiler filters), so that space is open. [GitHub](https://redirect.github.com/suhodolskiy/twitch-extension-chess)
- **Free single-game tools as SEO funnels.** Chessigma's free review plus utilities (an "Is chess.com down?" page and Elo calculators) bring in search traffic.

**Loops to build, ranked:**
1. **Monthly report card plus annual "Climb Wrapped".**
   - An animated, red-pen-styled story: biggest weakness fixed, best game, rating climbed, playstyle persona.
   - The persona is computed from our playstyle axes, never invented. Each claim is footnoted.
   - Export as an image and as an MP4 for Stories and TikTok.
   - Ship the annual one in late December. **Note:** today is 2026-11-21, so this is the launch window.
2. **"Moment of the month" clip.** An auto-generated short video of a player's best move or best save, with the board and eval graph animating. It is the shareable counterpart to "Brilliant".
3. **Streamer viewer reviews.** A streamer page and overlay: "Viewers' report cards". Viewers link accounts, and the streamer reviews ranked weaknesses on stream. That makes "roast my games" content. Give streamers affiliate codes.
4. **Free public "quick diagnosis"** by username, with no signup. It shows 3 area scores plus 1 costed weakness, and the full report is gated behind signup. This is the SEO and word-of-mouth landing page. Cost is bounded by reusing Lichess evals and a 20-game cap.
5. **Head-to-head and friend comparisons.** "You vs your club-mate: who converts better?" It only works with consent, and both players must opt in.
6. **Coach referrals.** Coaches invite students and see their progress, with consent.

---

## Recommendations for our design (ranked concrete additions and changes)

1. **Cap and shape the daily queue (§10.5, §12a).** Add a session-length picker (10, 20 or 40 minutes). Cap FSRS daily load. Never show backlog counts. Add a catch-up mode. The queue is built from the 3 active climb-plan weaknesses plus due reviews plus 1 post-game moment. *(Fixes the top Chessable and Chessbook complaint.)*
2. **Redesign streaks to be forgiving (§10.5).** Keep the streak separate from the daily goal. Make the minimum to keep a streak tiny (1 drill or the post-game moment). Allow 2 freezes. Offer an optional weekly-goal mode. Let the player hide the streak. Show the user's local-time day boundary. Send no guilt notifications. *(Duolingo data and chess.com complaints.)*
3. **Add win%-tolerant grading to every drill (§10.5, `chess-core`).** Accept moves within a configurable win% loss of best. Mark "only-move" puzzles explicitly. Give feedback like "Good. Best was Nf5 (+0.6)". *(Fixes the Aimchess harshness complaint.)*
4. **Make "transfer" the hero of Progress and the weekly email (§12a).** Show a before-and-after per targeted weakness, with confidence wording. Add one line to the top of the report as well. This is our main point of difference, so give it the prime spot.
5. **Add the instant post-game micro-review (new §10 item).** On sync, find 1 critical moment, write a grounded one-line explanation, and offer a 30-second retry. It is optional, and the push or email is opt-in. It counts toward the streak.
6. **Add the Maia human-likelihood layer (§8, §8a).** Mark each mistake as typical or unusual for the band. Weight opponent replies in the opening trainer by frequency at the band. Use a human-like sparring opponent for conversion and defence drills and endgame play. Write an ADR for licensing and hosting.
7. **Recency and filters for own-mistake puzzles (§10.5).** Default to the last 30 days. Add filters for time control, phase, motif and opening. Retire mastered items.
8. **Add "play the refutation" to every cited position (§10.3, §10.4).** Animate the engine PV with red-pen arrows. The explanation text stays tied to that PV through the validator.
9. **Add an evidence-linked "Ask your coach" chat.** It uses tool calls over the profile and DB only, uses the same claim validator, and has fair-use caps.
10. **Single-game review positioning (§12).** Make single-game review unlimited and run it client-side in WASM, so it costs almost nothing, to match Chessigma and Lichess. The cross-game diagnosis is the part that needs the server.
11. **Add a no-signup "quick diagnosis by username" landing flow (§10.1, §10.2).** Bounded to about 20 games, reusing Lichess evals.
12. **Add a monthly report card and an annual "Climb Wrapped" with video export (§10.8).** Every persona and statistic is grounded. Target the first one for late December 2026.
13. **Add "biggest gap" and "lines you actually reach" to the opening trainer (§8a).** Seed the repertoire from the player's own tree, not a blank one.
14. **Add a big-picture requirement to the game explorer (§10.4).** A persistent eval graph, a two-column clickable move list, and played and best moves side by side, on mobile too.
15. **Add a streamer programme.** A streamer overlay of "viewer report cards".
16. **Make drills offline-capable in the PWA (§3).** Cache the queue and the FSRS state, and sync when back online.
17. **Add a weekly "neglected area" nudge (improvement engine).** It shows only when a top-ranked weakness's area hasn't been trained for 14 days.
18. **Add opponent scouting for tournament prep (later).** Public games only, framed as prep, with no in-game surface, consistent with §3 non-goals.

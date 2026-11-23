# Legal and business research: Chess Improver

Date: 2026-11-21
Scope: third-party API terms, open-source licences, AI terms, privacy, competitor prices.
Rematch is free and open source, with no paid plans. Sections on its own pricing and payments were removed.
Context: `docs/superpowers/specs/2026-11-21-chess-improver-design.md`.

> **Not legal advice.** This is engineering research compiled from public sources on the date above. Terms change; several pages were read through automated fetch/summary tools, so wording quoted here should be re-checked against the live page before relying on it. Get a lawyer to review the ToS, privacy policy, and the chess.com position (section 2) before launch.

---

## TL;DR: things that affect the design

| # | Finding | Severity | Design impact |
|---|---|---|---|
| A | **chess.com User Agreement §4(D) (updated 25 Mar 2026)** bars using content to "develop or enhance chess engines, training models, **educational tools**, or game databases" and to build products that "compete with or substitute for Chess.com", without prior written authorisation. This conflicts with the more permissive PubAPI help article ("commercial purposes" OK if you don't copy features/IP). chess.com owns Aimchess, our direct competitor. | **High (potential blocker for chess.com import)** | Get written authorisation from chess.com Business Development before shipping chess.com-derived features, or launch Lichess-first with chess.com behind a flag. |
| B | **Lichess Opening Explorer requires an OAuth token on every request since 3 Mar 2026, limited to ~20 requests/minute per user.** | **High for §8.4 / §8a as written** | Cannot serve explorer stats for many users (or chess.com-only users) from one server token. Build our own rating-band explorer from the CC0 Lichess database (or self-host `lila-openingexplorer`, AGPL-3.0), and use per-user tokens only for live, user-initiated lookups. |
| C | Shipping stockfish.wasm to browsers **is distribution** under GPL-3.0. | Medium (compliance work, not a blocker) | Spec §16.4 is right in principle but needs a concrete checklist: corresponding source for the exact build, licence text, NNUE net, no obfuscation. |
| D | Many users will be minors. Anthropic's minors guidelines, COPPA (amended rule fully in force since 22 Apr 2026) and the UK Children's Code all apply to a chess product. | Medium-High | Age gate (block under-13), DPIA, high-privacy defaults for under-18s, public COPPA/minors statement, AI disclosure, child-safety system prompt. Review streak mechanics against Children's Code "nudge" standard. |
| E | Anthropic Commercial Terms §D.3 require telling users that factual assertions in outputs may be inaccurate; AUP requires AI disclosure for consumer-facing AI. | Low | Label reports "written by AI from Stockfish data" with an accuracy note. Fits the claim-validator design. |

Everything else (react-chessboard, chess.js, CC0 datasets, Lichess API for commercial use, Stockfish server-side) is clear.

---

## 1. Lichess API

### Commercial use: allowed
The Lichess Terms of Service (last modified 1 Aug 2026), section "Using our services":

> "Our services include (but are not limited to): API access, content, game databases, puzzle sets ... You can use our services for your own personal or commercial use, such as in your own applications, projects, research, or products."

But also:

> "All use of our services are subject to reasonable caps, limits, restrictions, or other bottle-necking which are purely determined at Lichess' discretion ... We also reserve the right to give privileged access to certain categories of our services (such as special API access) or revoke access temporarily or permanently as we see fit, at our sole discretion, and potentially without any advance warning."

So a commercial product is permitted, but there is no SLA and access can be revoked. There is no attribution requirement in the ToS for API data, though crediting Lichess is good manners and good for the relationship. Using multiple accounts to get around rate limits would breach the ToS.

- ToS: https://lichess.org/terms-of-service
- API docs: https://lichess.org/api
- API tips: https://lichess.org/page/api-tips

### Rate limits
- **"Only make one request at a time."** If you get HTTP 429, **"wait a full minute before resuming API usage"** (some limits need longer). Lichess will not publish exact per-endpoint limits because they "are varied and ever changing". https://lichess.org/page/api-tips
- **Game export streaming** (`/api/games/user/{username}`): anonymous 20 games/s; OAuth-authenticated 30 games/s; authenticated downloading **your own** games 60 games/s. A user who links via OAuth and exports their own games gets the fastest rate. Source: Lichess OpenAPI spec, `api-games-user-username` operation (https://github.com/lichess-org/api).
- Our worker must therefore run **one global in-flight request to lichess.org** (not one per user), or at most one per user token if Lichess confirms that's acceptable. Ask in `#lichess-api-support` on the Lichess Discord before launch and describe the expected volume.

### Opening explorer (important change)
Blog post "The opening explorer now requires authentication" (thibault, 3 Mar 2026; updated 19 Nov 2026): https://lichess.org/@/thibault/blog/the-opening-explorer-now-requires-authentication/FSWh9Zg3

- Anonymous requests to the Lichess-games explorer are no longer allowed, because of a sustained DDoS. "If you use the explorer through the API, you now need to add an oauth token to each explorer request."
- Limit: "you can send **20 requests per minute**", rate-limited at the Lichess **user** layer.
- Update 19 Nov 2026: the **masters** explorer is available anonymously again.
- The tablebase "may have to do the same thing at some point", but currently still serves anonymous requests.
- Lichess also says it doesn't want "commercial third-parties between chess players and us". That was about Cloudflare, but it signals sensitivity to intermediaries that put load on the explorer.

**Implications for the spec:**
- §8.4 ("left the move that Lichess Opening Explorer statistics favour at their rating band") and §8a "Replace" suggestions both need rating-band stats for many positions per user. At 20 req/min per token, that's too slow and depends on every user having a Lichess token. chess.com-only users have none, and routing them all through one service token would be the kind of "bottle-necking" abuse the ToS reserves the right to act on.
- **Recommended:** build our own explorer aggregate from the **CC0 Lichess open database** (https://database.lichess.org/, "Use them for research, commercial purpose, publication, anything you like"). Aggregate by rating band × time control × position to the depth we need (say 20–25 plies), stored in Postgres or a key-value store. Alternatively self-host `lila-openingexplorer` (AGPL-3.0, https://github.com/lichess-org/lila-openingexplorer). Running it unmodified as a separate internal service brings no source-disclosure obligation for our own code. If modified, offer its modified source.
- Keep live explorer calls only for user-initiated browsing, using that user's own token, with caching.

### Tablebase API
- `tablebase.lichess.org` still allows anonymous use, and the standard rules apply (one at a time, back off on 429). A Lichess moderator asked people not to work around limits because "the disks can only handle so much throughput". https://lichess.org/forum/lichess-feedback/does-the-web-tablebase-api-throttle
- Endgame drills involve a small, fixed set of positions plus user moves, so cache aggressively. For volume (such as tablebase-verifying every endgame in every imported game for the §8b KPI), consider **local Syzygy 3–5-piece tables (~1 GB) probed with Fathom (MIT)** and use the API only for 6–7-piece positions.

### OAuth
- PKCE public client, no app registration needed. Exporting public games needs no scope. Request the minimum scope set. Store tokens encrypted. Revoke on account deletion (`DELETE /api/token`).

---

## 2. chess.com Published-Data API (PubAPI)

### What the developer docs say (permissive)
Help article "What is the PubAPI and how do I use it?": https://support.chess.com/en/articles/9650547-what-is-the-pubapi-and-how-do-i-use-it

- "You may use our API, **including for commercial purposes**, as long as you do not abuse/spam our users, disrupt/harm our service, or **copy our features or IP**."
- "You must also clearly state that your product and/or service is **not affiliated with Chess.com**."
- IP to respect includes "board color palettes, piece designs, sound effects, **move classification glyphs**, **feature names ('Game Review', etc)**, UI/UX/design".
- Browser extensions should not provide engine analysis and are meant to be non-commercial. This doesn't apply to us, since we are a standalone web app, but it shows their sensitivity.
- Developers who want authentication or "public user data access" for a service are directed to a Business Development form.

Published-data API reference: https://www.chess.com/news/view/published-data-api

- Read-only, public data only.
- **Rate limits:** "Your serial access rate is unlimited." Parallel requests may get **429**. "If we detect abnormal or suspicious activity, we may block your application entirely." Supply a recognisable User-Agent with contact info, for example `my-tool/1.2 (username: x; contact: me@example.com)`.
- **Caching:** responses carry `ETag` / `Last-Modified`, so use conditional requests (304). Endpoints refresh at most every 12–24 hours, so polling a player's current-month archive more often than every ~12 h is wasted. This affects §12a's "every 3 hours for Pro": fine for Lichess, pointless for chess.com.
- Storing and caching data: no explicit prohibition in the PubAPI docs, and caching is expected.
- Daily Puzzle reuse needs a visible text-link credit. Not relevant unless we use it.

### What the User Agreement says (restrictive, potential conflict)
User Agreement, last updated **25 Mar 2026**: https://www.chess.com/legal/user-agreement

- **§4(D) Artificial Intelligence Restrictions:** "You may not use automated or artificial intelligence (AI) tools to access, scan, scrape, data mine, copy..." the content, including to "develop or enhance chess engines, training models, **educational tools**, or game databases". Other automated access or AI use "requires prior written authorization from Chess.com", granted or withheld at its sole discretion. It also prohibits using content to "create or improve any product or service that competes with or substitutes for Chess.com".
- §2(B): personal, non-commercial licence; no use of trademarks "without our prior permission".
- §2(C): game records (UGA) "can be accessed, used, and monetized by anyone for any number of purposes". This cuts the other way and is internally inconsistent with §2(D).
- §5(A): links must not create a false appearance of sponsorship.

**Assessment.** An AI-written coaching product built on chess.com game data, built in competition with chess.com's own Aimchess, Game Review and Insights, sits squarely within the text of §4(D). The PubAPI article's "commercial purposes" permission is the more specific developer-facing statement and is a reasonable basis to operate. But chess.com can block us at any time ("block your application entirely") and has a contractual hook to do so. Arguably the User Agreement binds chess.com *account holders*. We don't need a chess.com account to call the PubAPI, but our users who link their chess.com accounts are bound by it.

**History of blocking competitors.** I found no documented case of chess.com cutting off a competing analysis or coaching app's PubAPI access. Many developer-forum reports describe Cloudflare 403s from cloud/VPS IPs (for example DigitalOcean), sometimes even with a User-Agent: https://www.chess.com/clubs/forum/view/chess-com-api-returns-403-blocked-by-cloudflare. That's an operational risk for a Hetzner or Fly worker. Note also that chess.com is the seller of Aimchess on the App Store (seller "Chess.com, LLC"), so we compete directly with their product: https://apps.apple.com/us/app/aimchess-learn-chess-online/id1524941307

**Trademark / "connect your chess.com account".** Use plain text, no logo, no chess.com colours or glyphs. "Import your games from Chess.com" is safer wording than "Connect your chess.com account", since there is no account connection, just a username. Add: "Chess.com is a trademark of Chess.com, LLC. Chess Improver is not affiliated with or endorsed by Chess.com." Do **not** call any feature "Game Review", "Insights", "Brilliant (!!)", "Great Move" or "CAPS", and do not imitate their classification icons or green/cream board palette. Our planned labels (best, good, inaccuracy, mistake, blunder, missed win) are generic chess vocabulary and fine.

**Ownership verification** via the profile location field (spec §7) is a reasonable anti-abuse measure. It uses only public profile data.

---

## 3. Stockfish GPL-3.0

Stockfish README: "you MUST always include the license and the full source code (or a pointer to where the source code can be found) to generate the exact binary you are distributing. If you make any changes to the source code, these changes must also be made available under GPL v3." Its NNUE networks are "trained on data provided by the Leela Chess Zero project, which is made available under the Open Database License (ODbL)". https://github.com/official-stockfish/Stockfish

### (a) Server-side in a SaaS
GPL-3.0 (unlike AGPL-3.0) is triggered by **conveying** copies, not by network use. Running the native binary on our servers and sending users the results is not distribution, so there's no obligation to release our code, and none to release Stockfish source unless we ship the binary. This matches spec §14. Talking to it over UCI via stdin/stdout as a separate process is the classic "separate program" pattern (FSF GPL FAQ, "aggregate" / pipes). If we patch Stockfish server-side, we still have no obligation (no conveyance), but upstreaming is good citizenship.

Caveat: if the Docker image with Stockfish is ever given to anyone (a customer, contractor, or public registry), that *is* distribution.

### (b) stockfish.wasm in the browser
Sending the `.js` + `.wasm` (+ `.nnue`) to every visitor **is conveying object code**. That's fine, but these obligations apply:

1. Ship the GPL-3.0 licence text with it and show it in the UI (an "Engine: Stockfish NN, GPL-3.0, source" link in the about/licences page and in the engine panel).
2. Provide the **Corresponding Source** for the exact build: Stockfish C++ sources at the exact commit, the WASM port's glue (`stockfish-web` or `stockfish.js`), build scripts and Emscripten version/flags, and the NNUE net file(s) used. GPLv3 §6(d) allows offering it "from a designated place" (a URL) at no charge, kept available as long as we serve the object code. **Self-host a mirror** (a tagged tarball or our own fork) rather than relying only on upstream GitHub, because a moving upstream link doesn't identify the exact version.
3. Keep copyright/licence notices intact in the shipped files. Don't minify them away, and don't bundle the engine into our main JS bundle. Load it as a separate Web Worker asset.
4. Our app talks to it only via UCI strings over `postMessage`. That keeps it a separate program ("aggregate"), so our UI code stays proprietary. Don't link Stockfish C++ into our own WASM module or expose internal engine functions.
5. If we modify Stockfish (for example a custom eval or a stripped build), publish the modifications under GPL-3.0 with a prominent modification notice and date.
6. No additional restrictions: no DRM or anti-tamper on the engine files, and don't forbid users from extracting them.
7. Don't embed a proprietary NNUE net. The ChessBase/Fat Fritz 2 case ended in a 2022 settlement in which ChessBase admitted violating the GPL and stopped selling Fat Fritz 2 and Houdini 6. https://stockfishchess.org/blog/2022/chessbase-stockfish-agreement/ and https://fsfe.org/news/2022/news-20221212-01.html. Use the official nets, and credit Leela data (ODbL) on the licences page.

**What others do:** Lichess serves `lichess-org/stockfish-web` (GPL-3.0) in browsers and lists it on https://lichess.org/source, but Lichess is itself open source (AGPL). Chess.com maintains the `stockfish` npm package / `nmrugg/stockfish.js` (GPL-3.0 engine; non-engine glue MIT; copyright Chess.com, LLC), and runs it in its closed-source web app. That's precedent for "proprietary site + GPL WASM engine as a separate worker". https://github.com/nmrugg/stockfish.js, https://www.npmjs.com/package/stockfish. The `lite-single` build needs no COOP/COEP cross-origin isolation headers. The multi-threaded builds do, which affects Next.js header config and third-party embeds (Stripe.js and the like) under `require-corp`.

### Compliance checklist (Stockfish)
- [ ] Pin the exact engine build (package version + upstream commit + net hash) in an ADR.
- [ ] Serve engine files from `/engine/` as separate assets with notices intact. Not bundled or minified together with app code.
- [ ] Host a corresponding-source tarball (sources, build scripts, toolchain versions, nets) at a stable URL, linked from the engine UI and the licences page.
- [ ] `/licenses` page listing Stockfish (GPL-3.0 full text), the Leela data credit (ODbL), and all other OSS (generated from the dependency tree).
- [ ] UCI-over-postMessage only. No static linking with our code.
- [ ] Same treatment for any desktop or PWA offline bundle.
- [ ] Server images are not distributed. If that changes, include source or an offer.

---

## 4. Other component licences

| Component | Licence (verified via GitHub API / source page) | Obligation | Notes |
|---|---|---|---|
| `react-chessboard` (Clariity) | **MIT** | Keep the copyright notice in the shipped licence bundle | Spec choice is fine. |
| `chess.js` (jhlywa) | **BSD-2-Clause** | Keep notice | Fine. |
| `chessground` | GPL-3.0 | Would require GPL for the client | Correctly avoided (spec §11). |
| `chessops` (niklasf) | **GPL-3.0** | Same as chessground | Don't use it in the client or in proprietary server code you distribute. Server-only use is technically not distribution, but avoid it to keep things simple. |
| `lichess-org/chess-openings` | **CC0-1.0** | None | Credit is optional. |
| Lichess open database: games, **puzzles**, evals | **CC0** ("Use them for research, commercial purpose, publication, anything you like") https://database.lichess.org/ | None | The generator scripts repo (`lichess-org/database`) is AGPL-3.0, but that's the tooling, not the data. Puzzles include `PuzzleId`, and linking back to `lichess.org/training/{id}` is nice but optional. Puzzle and game data contain usernames (personal data; see §6). |
| Syzygy tablebase **files** | No restrictive licence asserted; freely redistributed. The generator (`syzygy1/tb`) is **GPL-2.0** | None for the data | Only relevant if we self-host. |
| Fathom (Syzygy prober) | **MIT** | Keep notice | Safe for local probing. |
| Lichess tablebase / explorer servers | AGPL-3.0 (`lila-tablebase`, `lila-openingexplorer`) | Only if self-hosted *and modified*: offer the modified source to network users | Our app code is unaffected because it's a separate service. |
| Stockfish | GPL-3.0 | See §3 | |
| Lichess piece sets / fonts | Various (some CC BY-SA, some GPL) | Check per asset | Don't take assets from the Lichess site without checking each licence. Use our own or MIT/CC0 sets. |

Action: generate a third-party licence file automatically (`license-checker` or similar) in CI, and fail the build on any GPL or AGPL in the web bundle except the isolated engine assets.

---

## 5. Anthropic (Claude API) terms

**Commercial Terms of Service** (effective 17 Jun 2025): https://www.anthropic.com/legal/commercial-terms
- §B: the customer "retains all rights to its Inputs" and "owns its Outputs". We can sell reports built from Claude output.
- §B: "Anthropic may not train models on Customer Content from Services."
- §D.3: we must **notify our users that factual assertions in Outputs may be inaccurate** and should be independently checked. Add a short note on the report ("AI-written explanation of engine data; engine figures are computed, prose may contain errors").
- §D.4: no building a competing AI product and no **reselling the Services** without approval. We sell a chess product that uses Claude, not raw API access, so this is fine. Don't expose a free-form "ask Claude anything" box that turns us into a Claude reseller.
- §K.1: Anthropic gives an IP indemnity for authorised use of Outputs.
- Data processing is under Anthropic's DPA, which is incorporated. Check the current API retention period and consider zero-data-retention if we send user data. We send only chess data plus a username, so data minimisation is easy.

**Usage Policy** (dated 12 Nov 2026): https://www.anthropic.com/legal/aup
- Consumer-facing AI that talks to users must disclose it is AI, "at a minimum at the beginning of each chat session or in the product interface". The reports aren't a chat, but label them as AI-written anyway, and do so in any future "ask the coach" chat.
- High-risk use cases (education grading that determines admission or certification, and so on) don't apply to recreational chess coaching.
- **Minors:** a minor is anyone under 18. Products that let minors interact directly with API-based products must follow the minors guidelines.

**Guidelines for organisations serving minors**: https://support.claude.com/en/articles/9307344-responsible-use-of-anthropic-s-models-guidelines-for-organizations-serving-minors
- Implement safety measures suited to the use case: age verification, content moderation and filtering, monitoring and reporting.
- Use Anthropic's child-safety system prompt if provided.
- Comply with COPPA and other child-privacy laws, and **state that compliance publicly on the website**.
- Disclose AI use. Anthropic may audit and can suspend accounts for non-compliance.
- Our design helps here: structured JSON output, no free chat, a claim validator, and input that is only chess data. Add an output moderation and length check, and document the safeguards on a "Safety & minors" page.

---

## 6. Privacy

### Data we hold
Account email; Lichess OAuth token; linked usernames; public games, which include **opponents'** usernames and ratings; derived profiles and weakness models; usage analytics; any shareable report cards.

Public game data is still **personal data** under GDPR when it is linked to a username, for our users and for their opponents. Our pooled benchmark statistics come from opponents' moves. That's a legitimate-interest use on public data. Disclose it in the privacy policy and keep benchmarks aggregated and anonymised.

### GDPR / UK GDPR basics (we have EU/UK users from day one; Art. 3(2) applies to non-EU businesses offering services there)
- **Lawful bases:** contract (Art. 6(1)(b)) for account, import, analysis and reports; legitimate interests for security, fraud prevention, benchmarking and product analytics (document a balancing test); consent for marketing email and non-essential cookies.
- **Profiling:** our weakness and playstyle profiling is not "solely automated decisions with legal or similarly significant effects" (Art. 22), but be transparent about it.
- **Transparency:** a privacy policy covering the controller identity, purposes, bases, recipients (Anthropic, hosting, email, Sentry), transfers, retention and rights.
- **Processors:** sign or accept DPAs with each one. International transfers rely on SCCs or the EU-US Data Privacy Framework where the vendor is certified.
- **EU representative:** Art. 27 requires one if we are not established in the EU (a UK representative likewise under UK GDPR), unless processing is "occasional", which ours is not. Cost is roughly €100–€500 per year through representative services. This depends on where the owner is based.
- **Rights:** access, export (offer JSON/PGN export), rectification, erasure, objection. Respond within one month.
- **Security:** encrypt OAuth tokens at rest (envelope encryption, key outside the DB); least-privilege; breach notification within 72 h to the supervisory authority.
- **Records:** keep a simple Art. 30 record of processing (likely required because processing is not occasional). Do a DPIA because of children (below).

### Children (the big one for chess)
- **GDPR Art. 8:** where we rely on consent for information-society services offered directly to a child, the child must be at least 16 unless a member state lowers this (13–16). Our core processing relies on *contract*, not consent, but the capacity of minors to contract varies by country.
- **UK Children's Code (Age Appropriate Design Code):** applies to services "**likely to be accessed by children**" (under 18), even if not aimed at them. https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/childrens-information/childrens-code-guidance-and-resources/age-appropriate-design-code/. That covers 15 standards: DPIA, high-privacy defaults, no profiling by default unless core to the service (our profiling *is* the core service, so document that), no "nudge techniques" that push lower privacy or extended use, and data minimisation. The ICO says self-declared age is no longer adequate where risk is high, and fined Reddit £14.7m in Feb 2026 over age assurance and risk assessment. Our risk is lower (no UGC or chat between users), but do a documented age-assurance risk assessment. Streaks and daily queues could be read as nudges, so give under-18s optional streaks, no loss-framing, and no push notifications by default.
- **COPPA (US, under 13):** applies if the service is directed to children or we have actual knowledge we're collecting data from under-13s. The amended rule took effect 23 Jun 2025, with compliance required from **22 Apr 2026** (separate written security programme, separate consent for third-party disclosure, data-retention limits). https://www.federalregister.gov/documents/2025/04/22/2025-05904/childrens-online-privacy-protection-rule. Recommendation: a general-audience service with a **neutral age gate**, asking date of birth without signalling the "right" answer. **Block under-13 sign-ups** (or build verifiable parental consent later as a "family/coach" feature). Avoid kid-directed marketing (cartoon mascots, school targeting) that could make us "directed to children". Note ChessKid exists for that market.
- Lichess's own ToS sets 15+ or the legal age (or parental authorisation) for accounts. Our users may still be young, especially chess.com users, where there's no OAuth age signal.
- **Recommended policy:** 13+ only. 13–17 allowed with a "high-privacy" profile: shareable report card off by default, no marketing email, and AI safeguards as in §5. 18+ normal.

### CCPA/CPRA (California)
Applies only if we exceed **$26,625,000** annual gross revenue (CPI-adjusted threshold, https://www.cppa.ca.gov/regulations/cpi_adjustment.html), or buy/sell/share personal information of 100,000+ consumers or households, or earn 50%+ of revenue from selling or sharing PI. **Likely not applicable at launch**, but other US state laws (some with lower thresholds or child-specific rules) are growing. Cheap wins: don't sell or share data, no ad-tech pixels, honour Global Privacy Control, and a "Do not sell/share" statement is trivially true.

### Cookies (ePrivacy / PECR)
- Strictly necessary cookies (session, CSRF) need no consent.
- Analytics: use a **cookieless, privacy-first analytics tool** (Plausible, self-hosted Umami, or server-side events) and avoid a consent banner entirely, or show a real opt-in banner. No Google Analytics or Meta pixel without opt-in. The UK Data (Use and Access) Act 2025 adds an exemption for some statistical cookies, but check commencement before relying on it.
- Sentry: turn off session replay or put it behind consent. Scrub PII.

### Deletion and retention
- In-app "Delete account": hard-delete user, linked accounts, tokens (and revoke the Lichess token), profiles, reports, drills and KPI snapshots within 30 days. Purge backups on a 30–35-day rotation, and document that.
- Imported games: delete the user's links to them. Shared, de-identified engine position cache (FEN → eval) and aggregated benchmarks can be retained because they're not personal data.
- Retention: delete inactive accounts after, say, 24 months with an email warning first.
- Public report cards: opt-in, revocable, `noindex` by default for minors.

---

## 7. Competitor prices

These describe other products, for context only.

### Benchmarks (check live pages; most are JS-rendered or vary by country)
| Product | Price | Notes |
|---|---|---|
| **Aimchess** (Chess.com, LLC) | **$9.99/mo or $71.99/yr** (App Store, US) https://apps.apple.com/us/app/aimchess-learn-chess-online/id1524941307 | The direct incumbent. Free tier is limited to about 15 lessons per day. Users complain about value and polish. |
| Chess.com Premium | Gold / Platinum / Diamond. Widely reported US list prices are about $29 / $49 / $99 per year (about $5–14 monthly). Pricing has varied by country since 1 Sep 2022 and was changed repeatedly in 2026. https://www.chess.com/membership | Diamond includes unlimited Game Review and Insights, our nearest substitute. Many of our target users already pay for it. |
| Lichess | Free (donations) | Free server analysis and an Insights-like tool set the expectation that "analysis" is free. Our value must be the **coaching and plan**, not the eval. |
| Chessable PRO | About $9.99/mo (unverified for 2026) | Course discounts. Opening training is a comparable feature. |
| ChessTempo, others | Low single-digit $/mo tiers (unverified) | Puzzles and opening training. |

---

## Required actions / decisions

**Blockers / design changes**
- [ ] **chess.com authorisation:** email chess.com Business Development (via the PubAPI help-article form) describing the product and requesting written confirmation that an AI-assisted coaching service using PubAPI data is acceptable under User Agreement §4(D). Decide the fallback if they refuse or don't answer: (a) launch with chess.com import behind a feature flag, (b) Lichess-first launch, or (c) proceed on the PubAPI article's permission and accept the risk. *Owner decision.*
- [ ] **Replace live Lichess Opening Explorer dependency** in §8.4 and §8a with our own rating-band explorer aggregate built from the CC0 Lichess database (or a self-hosted `lila-openingexplorer`). Use per-user OAuth explorer calls only for interactive browsing, rate-limited to under 20/min per token. Record this in an ADR.

**Compliance tasks before launch**
- [ ] Lichess: a single global request lane per host (one in flight), 60 s back-off on 429, identifying User-Agent with contact email, OAuth for own-game export (60 games/s), minimal scopes, token revoke on delete. Post in `#lichess-api-support` to describe expected volume.
- [ ] chess.com: strictly serial requests, ETag/If-None-Match conditional requests, poll no more than every 12 h, User-Agent with contact info, plan for Cloudflare blocks on datacentre IPs (test from the chosen host early).
- [ ] Branding: text-only platform names, a non-affiliation disclaimer in the footer and on the link page, no chess.com glyphs, palettes or feature names ("Game Review", "Insights", "Brilliant").
- [ ] Stockfish: complete the §3 checklist (pinned build, separate worker assets, hosted corresponding source, `/licenses` page, ODbL credit, UCI-only boundary). Decide on single-threaded `lite-single` (no COOP/COEP) or multi-threaded (needs cross-origin isolation headers).
- [ ] OSS licence scan in CI. Block GPL/AGPL from the app bundle (chessground, chessops). Generate a third-party notices file.
- [ ] Anthropic: an AI disclosure and accuracy note on every report (Commercial Terms §D.3, AUP); minors safeguards (child-safety system prompt, output moderation, public COPPA/minors statement); confirm API data retention and consider ZDR; never expose open-ended Claude access.
- [ ] Privacy: privacy policy, ToS, a DPIA (children plus profiling), Art. 30 record, DPAs with all processors, an EU/UK Art. 27 representative if the owner is outside the EU/UK, a security/retention policy (COPPA-style written programme is good practice even if we block under-13s).
- [ ] Age: a neutral DOB age gate. Block under-13s. 13–17 get high-privacy defaults (report card off, no marketing email, optional streaks, no push). *Owner decision on minimum age (13 recommended).*
- [ ] Cookies: cookieless analytics, no ad pixels, Sentry replay off. No banner needed, or a real opt-in banner if marketing tools are added.
- [ ] Deletion: one-click account deletion with a 30-day hard delete, backup rotation, a data export (JSON/PGN), and a retention schedule for inactive accounts.

**Spec corrections to log as ADRs**
- [ ] §7/§12a: chess.com data refreshes every 12–24 h, so the 3-hour poll only helps Lichess.
- [ ] §14: add the opening explorer authentication change; state that stockfish.wasm *is* distributed (spec §14 says "We don't distribute it", which is true only for the server binary); add the chess.com §4(D) risk.
- [ ] §8b endgame KPIs: use local Syzygy (Fathom, MIT) for bulk tablebase checks instead of the Lichess tablebase API.

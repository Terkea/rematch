-- The trainer's item refresh mark (ADR 0090 Amendment 1): pages never write on GET, so new drill
-- items are made by an explicit POST (or the next one), at most once per new set of analysed
-- games. `source_mark` summarises the scope's analysed games and profile rows when the items were
-- last made; a read compares it with the live summary to know whether new positions are pending.
CREATE TABLE drill_refreshes (
  user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  source_mark text NOT NULL,
  refreshed_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, account_id, time_class)
);

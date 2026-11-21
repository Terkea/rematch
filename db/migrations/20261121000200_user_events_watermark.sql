-- ADR 0034: the highest user_events id removed by pruning. A client whose Last-Event-ID is below
-- it may have missed events, so list_events_after returns a reset marker.
CREATE TABLE user_events_watermark (
  singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
  pruned_through_id bigint NOT NULL DEFAULT 0);
INSERT INTO user_events_watermark DEFAULT VALUES;

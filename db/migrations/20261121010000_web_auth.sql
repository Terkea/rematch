-- M4 web auth (ADR-WEB-3, ADR 0033): Better Auth 1.7 core tables with uuid ids, the age and privacy
-- columns on "user", and the invite allow-list for the private beta (ADR 0041).
-- Column names are the snake_case form of Better Auth's field names; the generated Drizzle schema
-- (packages/db/src/generated) maps them back to camelCase keys, which the drizzle adapter uses.

CREATE TABLE "user" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"name" text NOT NULL,
	"email" text NOT NULL,
	"email_verified" boolean DEFAULT false NOT NULL,
	"image" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	-- "YYYY-MM"; null until the age screen is answered (Task 20).
	"birth_month" text,
	"age_checked_at" timestamp with time zone,
	"marketing_email" boolean DEFAULT false NOT NULL,
	"public_share_default" boolean DEFAULT false NOT NULL,
	CONSTRAINT "user_email_unique" UNIQUE("email"),
	-- Better Auth lower-cases emails; the constraint keeps hand-written inserts honest too.
	CONSTRAINT "user_email_lower" CHECK ("email" = lower("email")),
	CONSTRAINT "user_birth_month_format" CHECK ("birth_month" IS NULL OR "birth_month" ~ '^[0-9]{4}-(0[1-9]|1[0-2])$')
);

CREATE TABLE "session" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"token" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	"ip_address" text,
	"user_agent" text,
	"user_id" uuid NOT NULL,
	CONSTRAINT "session_token_unique" UNIQUE("token")
);

CREATE TABLE "account" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"account_id" text NOT NULL,
	"provider_id" text NOT NULL,
	"user_id" uuid NOT NULL,
	-- access_token and refresh_token are encrypted by Better Auth (account.encryptOAuthTokens);
	-- id_token is stored as the provider sent it (a signed, non-secret identity assertion).
	"access_token" text,
	"refresh_token" text,
	"id_token" text,
	"access_token_expires_at" timestamp with time zone,
	"refresh_token_expires_at" timestamp with time zone,
	"scope" text,
	"password" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "account_provider_account_unique" UNIQUE("provider_id","account_id")
);

CREATE TABLE "verification" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"identifier" text NOT NULL,
	"value" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

-- Private beta allow-list (ADR 0041). Emails are stored lower-cased; INVITE_ONLY=0 disables the check.
CREATE TABLE "invites" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"email" text NOT NULL,
	"note" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "invites_email_unique" UNIQUE("email"),
	CONSTRAINT "invites_email_lower" CHECK ("email" = lower("email"))
);

ALTER TABLE "session" ADD CONSTRAINT "session_user_id_user_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."user"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "account" ADD CONSTRAINT "account_user_id_user_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."user"("id") ON DELETE cascade ON UPDATE no action;

CREATE INDEX "session_user_id_idx" ON "session" USING btree ("user_id");
CREATE INDEX "account_user_id_idx" ON "account" USING btree ("user_id");
CREATE INDEX "verification_identifier_idx" ON "verification" USING btree ("identifier");

-- Magic links actually sent, for the per-address throttle (ADR-WEB-3). Only sends are recorded, so
-- throttled requests never extend the lockout. Rows older than the window are deleted on insert.
CREATE TABLE "magic_link_sends" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"email" text NOT NULL,
	"sent_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE INDEX "magic_link_sends_email_sent_at_idx" ON "magic_link_sends" USING btree ("email", "sent_at");

-- Withdrawing an invite signs that person out everywhere: deleting the row, or changing its email,
-- deletes the sessions of the old address (ADR-WEB-3). The session guard then refuses new
-- sessions. Not covered: TRUNCATE (row triggers do not fire), and addresses that only come from
-- the INVITE_ALLOWLIST env var (removing one needs a restart and a manual sign-out).
CREATE FUNCTION "invites_revoke_sessions"() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
	IF TG_OP = 'DELETE' OR OLD."email" IS DISTINCT FROM NEW."email" THEN
		DELETE FROM "session" s USING "user" u WHERE s."user_id" = u."id" AND u."email" = OLD."email";
	END IF;
	RETURN NULL;
END;
$$;

CREATE TRIGGER "invites_revoke_sessions" AFTER DELETE OR UPDATE OF "email" ON "invites"
	FOR EACH ROW EXECUTE FUNCTION "invites_revoke_sessions"();

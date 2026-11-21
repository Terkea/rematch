-- M4 web account linking (Task 2's linking tables, built in Task 19; ADR-WEB-3, ADR-WEB-5).
-- A user reaches a platform account (linked_accounts, shared by every user who links it) only
-- through account_links. Any number of users may hold an unverified link to the same account, so
-- a squatter never blocks the real owner; at most one link per account is verified (partial unique
-- index), and the newest verification wins (the web app clears the previous owner in the same
-- transaction). linked_accounts.user_id stays for the M1 CLI only (deprecated for the web app).

CREATE TYPE "public"."link_method" AS ENUM('username', 'location_code', 'lichess_oauth', 'chesscom_oauth');

CREATE TABLE "account_links" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"linked_account_id" uuid NOT NULL,
	"method" "link_method" NOT NULL,
	"verified_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "account_links_user_account_unique" UNIQUE("user_id","linked_account_id")
);

ALTER TABLE "account_links" ADD CONSTRAINT "account_links_user_id_user_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."user"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "account_links" ADD CONSTRAINT "account_links_linked_account_id_linked_accounts_id_fk" FOREIGN KEY ("linked_account_id") REFERENCES "public"."linked_accounts"("id") ON DELETE no action ON UPDATE no action;

-- One verified owner per platform account.
CREATE UNIQUE INDEX "account_links_one_verified_owner" ON "account_links" USING btree ("linked_account_id") WHERE "verified_at" IS NOT NULL;

-- chess.com location-code verification (ADR-WEB-5): one pending code per link.
CREATE TABLE "chesscom_verifications" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"account_link_id" uuid NOT NULL,
	"code" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"last_checked_at" timestamp with time zone,
	"checks" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "chesscom_verifications_account_link_id_unique" UNIQUE("account_link_id"),
	CONSTRAINT "chesscom_verifications_code_format" CHECK ("code" ~ '^CI-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{6}$')
);

ALTER TABLE "chesscom_verifications" ADD CONSTRAINT "chesscom_verifications_account_link_id_account_links_id_fk" FOREIGN KEY ("account_link_id") REFERENCES "public"."account_links"("id") ON DELETE cascade ON UPDATE no action;

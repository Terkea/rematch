CREATE TYPE "public"."import_run_status" AS ENUM('running', 'done', 'failed');--> statement-breakpoint
CREATE TABLE "import_runs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"account_id" uuid NOT NULL,
	"status" "import_run_status" DEFAULT 'running' NOT NULL,
	"imported" integer DEFAULT 0 NOT NULL,
	"started_at" timestamp with time zone DEFAULT now() NOT NULL,
	"heartbeat_at" timestamp with time zone DEFAULT now() NOT NULL,
	"finished_at" timestamp with time zone,
	"last_error" text
);
--> statement-breakpoint
CREATE TABLE "user_events" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"account_id" uuid NOT NULL,
	"type" text NOT NULL,
	"payload" jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "games" ADD COLUMN "analysis_started_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "games" ADD COLUMN "heartbeat_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "games" ADD COLUMN "analysis_attempts" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "linked_accounts" ADD COLUMN "import_cursor" jsonb;--> statement-breakpoint
ALTER TABLE "import_runs" ADD CONSTRAINT "import_runs_account_id_linked_accounts_id_fk" FOREIGN KEY ("account_id") REFERENCES "public"."linked_accounts"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "import_runs_account_idx" ON "import_runs" USING btree ("account_id");--> statement-breakpoint
CREATE INDEX "import_runs_status_heartbeat_idx" ON "import_runs" USING btree ("status","heartbeat_at");--> statement-breakpoint
CREATE INDEX "user_events_account_id_idx" ON "user_events" USING btree ("account_id","id");--> statement-breakpoint
CREATE INDEX "user_events_created_idx" ON "user_events" USING btree ("created_at");--> statement-breakpoint
CREATE INDEX "games_analysis_status_idx" ON "games" USING btree ("analysis_status");
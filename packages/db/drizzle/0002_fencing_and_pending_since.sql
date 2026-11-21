ALTER TYPE "public"."import_run_status" ADD VALUE 'paused';--> statement-breakpoint
ALTER TABLE "games" ADD COLUMN "pending_since" timestamp with time zone DEFAULT now();--> statement-breakpoint
ALTER TABLE "import_runs" ADD COLUMN "error_code" text;
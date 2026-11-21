CREATE TYPE "public"."analysis_status" AS ENUM('pending', 'analysing', 'done', 'failed', 'skipped');--> statement-breakpoint
CREATE TYPE "public"."color" AS ENUM('white', 'black');--> statement-breakpoint
CREATE TYPE "public"."eval_source" AS ENUM('stockfish', 'lichess', 'dump');--> statement-breakpoint
CREATE TYPE "public"."judgement" AS ENUM('inaccuracy', 'mistake', 'blunder');--> statement-breakpoint
CREATE TYPE "public"."phase" AS ENUM('opening', 'middlegame', 'endgame');--> statement-breakpoint
CREATE TYPE "public"."platform" AS ENUM('lichess', 'chesscom');--> statement-breakpoint
CREATE TYPE "public"."result" AS ENUM('1-0', '0-1', '1/2-1/2', '*');--> statement-breakpoint
CREATE TYPE "public"."time_class" AS ENUM('bullet', 'blitz', 'rapid', 'classical', 'daily');--> statement-breakpoint
CREATE TABLE "games" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"account_id" uuid NOT NULL,
	"platform" "platform" NOT NULL,
	"platform_game_id" text NOT NULL,
	"pgn" text NOT NULL,
	"played_at" timestamp with time zone NOT NULL,
	"time_class" time_class NOT NULL,
	"base_seconds" integer,
	"increment_seconds" integer,
	"rated" boolean NOT NULL,
	"player_color" "color" NOT NULL,
	"white_name" text NOT NULL,
	"black_name" text NOT NULL,
	"white_rating" integer,
	"black_rating" integer,
	"result" "result" NOT NULL,
	"termination" text,
	"eco" text,
	"opening_name" text,
	"lichess_evals" jsonb,
	"analysis_status" "analysis_status" DEFAULT 'pending' NOT NULL,
	"analysis_protocol" text,
	"analysis_error" text,
	"white_accuracy" real,
	"black_accuracy" real,
	CONSTRAINT "games_platform_game_uq" UNIQUE("platform","platform_game_id")
);
--> statement-breakpoint
CREATE TABLE "linked_accounts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid,
	"platform" "platform" NOT NULL,
	"username" text NOT NULL,
	"display_name" text NOT NULL,
	"verified_at" timestamp with time zone,
	"last_synced_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "linked_accounts_platform_username_uq" UNIQUE("platform","username")
);
--> statement-breakpoint
CREATE TABLE "moves" (
	"game_id" uuid NOT NULL,
	"ply" integer NOT NULL,
	"color" "color" NOT NULL,
	"san" text NOT NULL,
	"uci" text NOT NULL,
	"fen_before" text NOT NULL,
	"best_uci" text,
	"win_before" double precision NOT NULL,
	"win_after" double precision NOT NULL,
	"delta_w" double precision NOT NULL,
	"accuracy" double precision NOT NULL,
	"judgement" "judgement",
	"phase" "phase" NOT NULL,
	"clock_before" double precision,
	"time_spent" double precision,
	"decided" boolean NOT NULL,
	CONSTRAINT "moves_game_id_ply_pk" PRIMARY KEY("game_id","ply")
);
--> statement-breakpoint
CREATE TABLE "position_evals" (
	"fen_key" text NOT NULL,
	"protocol" text NOT NULL,
	"score_cp" integer,
	"score_mate" integer,
	"best_uci" text,
	"pv" text[] DEFAULT '{}'::text[] NOT NULL,
	"source" "eval_source" NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "position_evals_fen_key_protocol_pk" PRIMARY KEY("fen_key","protocol")
);
--> statement-breakpoint
ALTER TABLE "games" ADD CONSTRAINT "games_account_id_linked_accounts_id_fk" FOREIGN KEY ("account_id") REFERENCES "public"."linked_accounts"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "moves" ADD CONSTRAINT "moves_game_id_games_id_fk" FOREIGN KEY ("game_id") REFERENCES "public"."games"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "games_account_played_idx" ON "games" USING btree ("account_id","played_at" DESC NULLS LAST);
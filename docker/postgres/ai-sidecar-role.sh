#!/bin/sh
# Dev only (ADR-R3-6). The migration creates the ai_sidecar role NOLOGIN, with access to
# ai_sdk_session_entries only. This grants it LOGIN and the password from AI_SIDECAR_DB_PASSWORD,
# so the sidecar can connect with
#   AI_SESSION_DATABASE_URL=postgres://ai_sidecar:<password>@postgres:5432/chess
# Run it once after the migrations have been applied (re-running just resets the password):
#   docker compose --profile ai run --rm ai-sidecar-role
# Production grants LOGIN and the password through its own secret management, never this script.
set -eu
: "${AI_SIDECAR_DB_PASSWORD:?set AI_SIDECAR_DB_PASSWORD (16+ characters) in .env}"
if [ "${#AI_SIDECAR_DB_PASSWORD}" -lt 16 ]; then
  echo "AI_SIDECAR_DB_PASSWORD must be at least 16 characters" >&2
  exit 1
fi
# \getenv reads the password from the environment (never argv), and :'pw' quotes it as a literal.
# ALTER ROLE fails clearly if the migration has not created the role yet.
psql -q -v ON_ERROR_STOP=1 <<'SQL'
\getenv pw AI_SIDECAR_DB_PASSWORD
ALTER ROLE ai_sidecar LOGIN PASSWORD :'pw';
\echo 'ai_sidecar: LOGIN granted'
SQL

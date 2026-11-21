#!/bin/sh
set -e
# The `deps` service installs once; services that depend on it set SKIP_INSTALL=1 so two installs
# never race on the shared volumes.
if [ "${SKIP_INSTALL:-}" != "1" ]; then
  pnpm install --frozen-lockfile
fi
exec "$@"

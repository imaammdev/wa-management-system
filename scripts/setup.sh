#!/usr/bin/env bash
# One-time setup: writes .env with a fresh encryption key and checks the
# database connection. Safe to re-run; it never overwrites an existing .env.
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="$HOME/.cargo/bin:$PATH"

if [ -f .env ]; then
  echo "✓ .env already exists, leaving it alone"
else
  key=$(openssl rand -hex 32)
  sed "s/^ENCRYPTION_KEY=.*/ENCRYPTION_KEY=$key/" .env.example > .env
  echo "✓ wrote .env with a fresh ENCRYPTION_KEY"
fi

# shellcheck disable=SC1091
set -a; . ./.env; set +a

echo "→ checking database at ${DATABASE_URL%%\?*}"
if command -v psql >/dev/null 2>&1; then
  if psql "$DATABASE_URL" -c 'SELECT 1' >/dev/null 2>&1; then
    echo "✓ database reachable"
  else
    echo "✗ cannot connect. Create the role and database first, e.g.:"
    echo "    createuser -s mms && createdb -O mms mms"
    exit 1
  fi
else
  echo "! psql not on PATH — skipping the connection check."
  echo "  The API applies migrations on boot, so it will tell you if the URL is wrong."
fi

echo
echo "Next:  ./scripts/dev.sh"

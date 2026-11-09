#!/usr/bin/env bash
# End-to-end smoke test against a running API, using the mock provider.
# Registers a user, connects a mock number, sends a message, simulates an
# inbound reply and checks it landed in the inbox.
set -euo pipefail

BASE="${BASE:-http://127.0.0.1:8080}"
JAR=$(mktemp)
trap 'rm -f "$JAR"' EXIT
EMAIL="smoke+$RANDOM@example.com"

j() { python3 -c 'import sys,json;d=json.load(sys.stdin);
import functools
p=sys.argv[1].split(".")
for k in p: d=d[int(k)] if k.isdigit() else d[k]
print(d)' "$1"; }

say() { printf '\n\033[1m%s\033[0m\n' "$1"; }

say "1. health"
curl -sf "$BASE/healthz" | tee /dev/stderr | grep -q '"status":"ok"'

say "2. register"
curl -sf -c "$JAR" -X POST "$BASE/api/auth/register" \
  -H 'content-type: application/json' \
  -d "{\"name\":\"Smoke\",\"email\":\"$EMAIL\",\"password\":\"rahasia123\",\"org_name\":\"Smoke Co\"}" \
  > /tmp/reg.json
ORG=$(j organization.id < /tmp/reg.json)
echo "org=$ORG"

say "3. create mock number"
curl -sf -b "$JAR" -X POST "$BASE/api/whatsapp-accounts" \
  -H 'content-type: application/json' -H "x-org-id: $ORG" \
  -d '{"label":"Smoke CS","provider":"mock"}' > /tmp/num.json
NUM=$(j whatsapp_account.id < /tmp/num.json)
echo "number=$NUM"

say "4. connect it"
curl -sf -b "$JAR" -X POST "$BASE/api/whatsapp-accounts/$NUM/connect" \
  -H 'content-type: application/json' -H "x-org-id: $ORG" \
  -d "{\"waba_id\":\"MOCK\",\"phone_number_id\":\"mock-$NUM\",\"access_token\":\"tok\"}" \
  | grep -q '"status":"connected"'

say "5. simulate an inbound message (opens the 24h window)"
curl -sf -b "$JAR" -X POST "$BASE/api/whatsapp-accounts/$NUM/simulate-inbound" \
  -H 'content-type: application/json' -H "x-org-id: $ORG" \
  -d '{"from":"6281234567890","text":"Halo, mau tanya harga"}' \
  | grep -q '"stored":1'

say "6. mint an API key"
curl -sf -b "$JAR" -X POST "$BASE/api/api-keys" \
  -H 'content-type: application/json' -H "x-org-id: $ORG" \
  -d '{"name":"smoke"}' > /tmp/key.json
KEY=$(j secret < /tmp/key.json)
echo "key=${KEY:0:20}…"

say "7. send a reply through the public /v1 API"
curl -sf -X POST "$BASE/v1/messages" \
  -H "authorization: Bearer $KEY" \
  -H 'content-type: application/json' \
  -H "idempotency-key: smoke-$RANDOM" \
  -d "{\"wa_account_id\":\"$NUM\",\"to\":\"6281234567890\",\"type\":\"text\",\"body\":\"Halo! Ada yang bisa dibantu?\"}" \
  | tee /dev/stderr | grep -q '"status":"sent"'

say "8. conversation shows up in the inbox"
curl -sf -b "$JAR" "$BASE/api/inbox/conversations" -H "x-org-id: $ORG" \
  | grep -q '6281234567890'

say "9. quota was metered"
curl -sf -b "$JAR" "$BASE/api/organizations/current/usage" -H "x-org-id: $ORG" \
  | tee /dev/stderr | grep -q '"messages_sent":1'

printf '\n\033[32m✓ smoke test passed\033[0m\n'

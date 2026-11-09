# MMS — WhatsApp Business API Platform

A multi-tenant platform that wraps Meta's official **WhatsApp Cloud API** and
exposes it as a developer-facing REST API plus a shared inbox for CS teams.

Rust (Axum) API · Postgres · React dashboard · Bahasa Indonesia UI · IDR pricing.

---

## What it does

| Area | Shipped |
|---|---|
| **Public API** | `POST /v1/messages` (text, image, video, audio, document, location, template, interactive, reaction), `GET /v1/messages/:id`, contacts, templates, numbers. Bearer API keys with scopes, per-key rate limiting, `Idempotency-Key` support. |
| **Inbound** | One webhook URL for every hosted number. `X-Hub-Signature-256` verification, raw-payload dedup, 24h service-window tracking, delivery receipts applied monotonically. |
| **Outbound webhooks** | `message.received` / `message.status` fan-out per endpoint, HMAC-SHA256 signed, 8 attempts with exponential backoff, full delivery log, one-click replay. |
| **Shared inbox** | Conversations, assignment, labels, pipelines, notes, canned responses, unread counts, 24h window enforcement in the UI. |
| **Broadcasts** | Approved-template campaigns, materialised audiences, per-minute throttling, opt-out respected, resumable in batches. |
| **Multi-tenancy** | Organization → Customers (agency sub-clients) → Teams → Members. 4-role RBAC. API keys scopable to a single sub-client. |
| **Billing** | Plans with hard quotas (messages, broadcasts, numbers, members, storage, history retention), monthly usage metering, IDR pricing, server-side cost calculator including Meta's own per-message rates. |
| **Security** | Argon2 passwords, AES-256-GCM sealed provider tokens, SHA-256 API-key hashes with constant-time comparison, audit log table. |

## Architecture

```
crates/
  core/       domain + Postgres access. Tenant scope is always an explicit argument.
  provider/   WaProvider trait → meta_cloud (Graph API) | mock (local, no Meta needed)
  api/        Axum: /v1 (API keys) · /api (session cookie) · /webhooks (Meta) · /public
  worker/     drains the `jobs` table: webhook delivery, broadcasts, retention
migrations/   7 SQL files, ~30 tables
web/          React + Vite dashboard and landing page
packages/sdk/ TypeScript SDK + webhook signature verification
```

Two deliberate choices:

- **The job queue is Postgres**, not Redis (`FOR UPDATE SKIP LOCKED`). Enqueueing
  a webhook and writing the message that triggered it share one transaction, so
  an event can never be queued for a write that rolled back.
- **One send path.** API calls, inbox replies, broadcasts and the AI agent all go
  through `service::send`, so quota, the 24-hour window rule and delivery
  bookkeeping cannot be bypassed by adding a new caller.

## The mock provider

`WA_PROVIDER=mock` runs the entire platform with **no Meta app, no WABA and no
billing**. It generates Meta-shaped webhook payloads that are parsed by the same
code as production traffic, so the inbox, webhooks, broadcasts and quota
metering are all exercisable before your Meta access is approved. Numbers can be
mock and real side by side in one deployment — the adapter is chosen per number.

## Setup

Needs: Rust (installed at `~/.cargo/bin`), Node + pnpm, and a Postgres 14+ database.

```bash
./scripts/setup.sh     # writes .env with a fresh ENCRYPTION_KEY, checks the DB
./scripts/dev.sh       # api :8080 · worker · dashboard :5173
./scripts/smoke.sh     # end-to-end check against the running API
```

Migrations run automatically when the API boots.

### Database

Any Postgres works. Set `DATABASE_URL` in `.env`:

```
# local
postgres://mms:mms@127.0.0.1:5432/mms
# hosted (Neon, Supabase, RDS…)
postgres://user:pass@host/db?sslmode=require
```

## Connecting a real number

1. Meta Business Manager → create an app with the WhatsApp product, add a phone
   number, and create a **system user token** with `whatsapp_business_messaging`
   and `whatsapp_business_management`.
2. Set `META_APP_SECRET` in `.env`. Once set, inbound webhook signatures are
   **required** — unsigned calls are rejected with 401.
3. Point Meta's webhook at `https://your-host/webhooks/meta` using
   `META_WEBHOOK_VERIFY_TOKEN`, and subscribe to the `messages` field.
4. Dashboard → *Nomor WhatsApp* → add a `meta_cloud` number → **Hubungkan**, and
   paste the WABA id, phone number id and token. The credentials are verified
   against the Graph API before they are stored, so a number never sits in
   `connected` with a token that cannot send.

## API example

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer mms_live_…" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{
    "wa_account_id": "…",
    "to": "628123456789",
    "type": "text",
    "body": "Halo!"
  }'
```

Errors are consistent and machine-readable:

```json
{ "error": { "code": "quota_exceeded", "message": "…",
             "details": { "metric": "messages_per_month", "used": 200000, "limit": 200000 } } }
```

`402` quota · `422` validation · `429` rate limit (with `retry_after`) ·
`502` upstream provider. 5xx bodies are deliberately vague; detail goes to the log.

## Webhook verification

```ts
import { verifyWebhookSignature } from "@mms/sdk";

// Must be the RAW body — a re-serialized object won't match.
const ok = await verifyWebhookSignature(
  process.env.MMS_WEBHOOK_SECRET!,
  rawBody,
  req.headers["x-mms-signature-256"] as string,
);
```

## Tests

```bash
cargo test --workspace      # 23 unit tests
cd web && pnpm build        # strict TypeScript + production bundle
```

Covered: crypto round-trips (seal/open, password, API-key prefix↔hash, HMAC),
RBAC and cross-tenant isolation, Meta webhook parsing (including malformed
payloads), Cloud API request building, rate-limiter behaviour, retry backoff,
phone normalization, and broadcast variable substitution.

## Not built yet

- Payment capture (Midtrans/Xendit). Plan changes apply immediately; the
  provider seam is `routes/billing.rs`.
- Transactional email. Invites return a link for the admin to share.
- AI agent execution. Schema (`ai_agents`) and the job kind exist; the inference
  call does not.
- Media upload/proxy. `media_objects` and `media_url()` exist; no upload route.
- Embedded Signup (requires Meta Tech Provider status). Manual connect works.
- MCP server and n8n node.
- Realtime is 5s polling, not SSE/websockets.

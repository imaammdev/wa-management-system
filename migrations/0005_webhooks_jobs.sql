-- 0005_webhooks_jobs.sql — outbound webhooks with retry/replay + job queue
CREATE TABLE webhook_endpoints (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  customer_id UUID REFERENCES customers(id) ON DELETE CASCADE,
  url        TEXT NOT NULL,
  secret     TEXT NOT NULL,           -- HMAC-SHA256 signing secret
  events     TEXT[] NOT NULL DEFAULT ARRAY['message.received','message.status']::TEXT[],
  active     BOOLEAN NOT NULL DEFAULT true,
  description TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX webhook_endpoints_org_idx ON webhook_endpoints(org_id);

CREATE TABLE webhook_deliveries (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  endpoint_id UUID NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
  event_id    UUID NOT NULL,          -- stable across retries, used for dedup
  event_type  TEXT NOT NULL,
  payload     JSONB NOT NULL,
  status      TEXT NOT NULL DEFAULT 'pending'
              CHECK (status IN ('pending','delivered','failed','exhausted')),
  attempt     INT NOT NULL DEFAULT 0,
  max_attempts INT NOT NULL DEFAULT 8,
  next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  response_status INT,
  response_body   TEXT,
  duration_ms  INT,
  last_error   TEXT,
  delivered_at TIMESTAMPTZ,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX webhook_deliveries_due_idx
  ON webhook_deliveries(next_attempt_at) WHERE status = 'pending';
CREATE INDEX webhook_deliveries_org_idx ON webhook_deliveries(org_id, created_at DESC);
CREATE INDEX webhook_deliveries_endpoint_idx ON webhook_deliveries(endpoint_id, created_at DESC);

-- Raw inbound payloads from Meta, kept for dedup + replay/debugging.
CREATE TABLE inbound_events (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  wa_account_id UUID REFERENCES whatsapp_accounts(id) ON DELETE SET NULL,
  provider      TEXT NOT NULL DEFAULT 'meta_cloud',
  dedup_key     TEXT NOT NULL UNIQUE,
  payload       JSONB NOT NULL,
  signature_ok  BOOLEAN NOT NULL DEFAULT false,
  processed_at   TIMESTAMPTZ,
  process_error  TEXT,
  received_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX inbound_events_received_idx ON inbound_events(received_at DESC);

-- Postgres-backed queue (FOR UPDATE SKIP LOCKED). No Redis needed.
CREATE TABLE jobs (
  id           BIGSERIAL PRIMARY KEY,
  kind         TEXT NOT NULL,
  org_id       UUID REFERENCES organizations(id) ON DELETE CASCADE,
  payload      JSONB NOT NULL DEFAULT '{}'::jsonb,
  status       TEXT NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending','running','done','failed')),
  priority     INT NOT NULL DEFAULT 0,
  attempt      INT NOT NULL DEFAULT 0,
  max_attempts INT NOT NULL DEFAULT 5,
  run_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  locked_at    TIMESTAMPTZ,
  locked_by    TEXT,
  last_error   TEXT,
  dedup_key    TEXT,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at  TIMESTAMPTZ
);
CREATE INDEX jobs_claim_idx ON jobs(status, run_at, priority DESC)
  WHERE status = 'pending';
CREATE UNIQUE INDEX jobs_dedup_uidx ON jobs(dedup_key)
  WHERE dedup_key IS NOT NULL AND status IN ('pending','running');

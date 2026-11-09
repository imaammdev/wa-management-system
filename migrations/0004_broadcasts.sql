-- 0004_broadcasts.sql — campaigns, audiences, AI agents
CREATE TABLE segments (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  -- {"all":true} | {"label_ids":[...]} | {"attributes":{"city":"Bandung"}}
  filter     JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (org_id, name)
);

CREATE TABLE broadcasts (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  customer_id   UUID REFERENCES customers(id) ON DELETE CASCADE,
  wa_account_id UUID NOT NULL REFERENCES whatsapp_accounts(id) ON DELETE CASCADE,
  template_id   UUID REFERENCES message_templates(id) ON DELETE SET NULL,
  segment_id    UUID REFERENCES segments(id) ON DELETE SET NULL,
  name          TEXT NOT NULL,
  -- per-recipient template variable mapping, e.g. {"1":"{{contact.name}}"}
  variables     JSONB NOT NULL DEFAULT '{}'::jsonb,
  status        TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','scheduled','running','paused','completed','cancelled','failed')),
  throttle_per_minute INT NOT NULL DEFAULT 600,
  scheduled_at  TIMESTAMPTZ,
  started_at    TIMESTAMPTZ,
  completed_at  TIMESTAMPTZ,
  total_recipients INT NOT NULL DEFAULT 0,
  sent_count    INT NOT NULL DEFAULT 0,
  delivered_count INT NOT NULL DEFAULT 0,
  read_count    INT NOT NULL DEFAULT 0,
  failed_count  INT NOT NULL DEFAULT 0,
  created_by    UUID REFERENCES users(id) ON DELETE SET NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX broadcasts_org_idx ON broadcasts(org_id, created_at DESC);
CREATE INDEX broadcasts_due_idx ON broadcasts(status, scheduled_at)
  WHERE status IN ('scheduled','running');

CREATE TABLE broadcast_recipients (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON DELETE CASCADE,
  contact_id   UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  message_id   UUID REFERENCES messages(id) ON DELETE SET NULL,
  status       TEXT NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending','sent','delivered','read','failed','skipped')),
  error_code   TEXT,
  error_detail TEXT,
  processed_at TIMESTAMPTZ,
  UNIQUE (broadcast_id, contact_id)
);
CREATE INDEX broadcast_recipients_pending_idx
  ON broadcast_recipients(broadcast_id) WHERE status = 'pending';

CREATE TABLE ai_agents (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  wa_account_id UUID REFERENCES whatsapp_accounts(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  model         TEXT NOT NULL DEFAULT 'claude-sonnet-5-5',
  system_prompt TEXT NOT NULL DEFAULT '',
  knowledge     JSONB NOT NULL DEFAULT '[]'::jsonb,
  -- {"on_intent":["pricing"],"after_turns":6,"on_keyword":["manusia","agent"]}
  handoff_rules JSONB NOT NULL DEFAULT '{}'::jsonb,
  enabled       BOOLEAN NOT NULL DEFAULT false,
  office_hours  JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

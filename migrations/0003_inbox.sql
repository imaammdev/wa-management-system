-- 0003_inbox.sql — contacts, conversations, messages, labels, pipelines
CREATE TABLE contacts (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  customer_id UUID REFERENCES customers(id) ON DELETE CASCADE,
  wa_id       TEXT NOT NULL,              -- E.164 without '+', e.g. 628123456789
  name        TEXT,
  profile_name TEXT,
  email       CITEXT,
  attributes  JSONB NOT NULL DEFAULT '{}'::jsonb,
  opt_in_status TEXT NOT NULL DEFAULT 'unknown'
                CHECK (opt_in_status IN ('unknown','opted_in','opted_out')),
  opt_in_source TEXT,
  blocked_at  TIMESTAMPTZ,
  last_seen_at TIMESTAMPTZ,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- one contact row per phone per tenant scope (customer_id NULL = org-level)
CREATE UNIQUE INDEX contacts_scope_wa_uidx
  ON contacts(org_id, COALESCE(customer_id, '00000000-0000-0000-0000-000000000000'::uuid), wa_id);
CREATE INDEX contacts_org_idx ON contacts(org_id);
CREATE INDEX contacts_name_trgm_idx ON contacts(org_id, name);

CREATE TABLE labels (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  color      TEXT NOT NULL DEFAULT '#64748b',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (org_id, name)
);

CREATE TABLE pipelines (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  is_default BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (org_id, name)
);

CREATE TABLE pipeline_stages (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  position    INT NOT NULL DEFAULT 0,
  UNIQUE (pipeline_id, name)
);

CREATE TABLE conversations (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  customer_id   UUID REFERENCES customers(id) ON DELETE CASCADE,
  wa_account_id UUID NOT NULL REFERENCES whatsapp_accounts(id) ON DELETE CASCADE,
  contact_id    UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  status        TEXT NOT NULL DEFAULT 'open'
                CHECK (status IN ('open','pending','snoozed','closed')),
  assigned_to   UUID REFERENCES users(id) ON DELETE SET NULL,
  assigned_team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
  stage_id      UUID REFERENCES pipeline_stages(id) ON DELETE SET NULL,
  -- Meta 24h customer service window; NULL or past = template-only
  service_window_expires_at TIMESTAMPTZ,
  ai_agent_enabled BOOLEAN NOT NULL DEFAULT false,
  unread_count  INT NOT NULL DEFAULT 0,
  last_message_at TIMESTAMPTZ,
  last_message_preview TEXT,
  snoozed_until TIMESTAMPTZ,
  closed_at     TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (wa_account_id, contact_id)
);
CREATE INDEX conversations_inbox_idx
  ON conversations(org_id, status, last_message_at DESC NULLS LAST);
CREATE INDEX conversations_assigned_idx ON conversations(org_id, assigned_to);

CREATE TABLE conversation_labels (
  conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  label_id        UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (conversation_id, label_id)
);

CREATE TABLE messages (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  customer_id   UUID REFERENCES customers(id) ON DELETE CASCADE,
  conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  wa_account_id UUID NOT NULL REFERENCES whatsapp_accounts(id) ON DELETE CASCADE,
  contact_id    UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  direction     TEXT NOT NULL CHECK (direction IN ('inbound','outbound')),
  wa_message_id TEXT,
  reply_to_wa_message_id TEXT,
  msg_type      TEXT NOT NULL
                CHECK (msg_type IN ('text','image','video','audio','document',
                                    'sticker','location','contacts','template',
                                    'interactive','reaction','system','unsupported')),
  body          TEXT,
  payload       JSONB NOT NULL DEFAULT '{}'::jsonb,
  media_id      UUID REFERENCES media_objects(id) ON DELETE SET NULL,
  template_id   UUID REFERENCES message_templates(id) ON DELETE SET NULL,
  status        TEXT NOT NULL DEFAULT 'queued'
                CHECK (status IN ('queued','sent','delivered','read','failed','deleted')),
  error_code    TEXT,
  error_detail  TEXT,
  pricing_category TEXT
                CHECK (pricing_category IN ('marketing','utility','authentication','service')),
  billable      BOOLEAN,
  -- who/what produced it
  source        TEXT NOT NULL DEFAULT 'api'
                CHECK (source IN ('api','dashboard','broadcast','ai_agent','system')),
  sent_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
  api_key_id    UUID REFERENCES api_keys(id) ON DELETE SET NULL,
  idempotency_key TEXT,
  sent_at       TIMESTAMPTZ,
  delivered_at  TIMESTAMPTZ,
  read_at       TIMESTAMPTZ,
  failed_at     TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX messages_wa_id_uidx
  ON messages(wa_account_id, wa_message_id) WHERE wa_message_id IS NOT NULL;
CREATE UNIQUE INDEX messages_idempotency_uidx
  ON messages(org_id, idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX messages_conversation_idx ON messages(conversation_id, created_at DESC);
CREATE INDEX messages_org_created_idx ON messages(org_id, created_at DESC);

CREATE TABLE conversation_notes (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  author_id  UUID REFERENCES users(id) ON DELETE SET NULL,
  body       TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE canned_responses (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  shortcut   TEXT NOT NULL,
  body       TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (org_id, shortcut)
);

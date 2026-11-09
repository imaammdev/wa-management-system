-- 0008_inbox_crm.sql — inbox triage (priority, hold, reply tracking),
-- contact labels, activity timeline, deals, and Meta lead events.

-- ---------------------------------------------------------------- triage
-- 'held' joins the lifecycle: parked deliberately, as opposed to 'snoozed'
-- which wakes itself up again.
ALTER TABLE conversations DROP CONSTRAINT IF EXISTS conversations_status_check;
ALTER TABLE conversations
  ADD CONSTRAINT conversations_status_check
  CHECK (status IN ('open','pending','held','snoozed','closed'));

ALTER TABLE conversations
  ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'medium'
    CHECK (priority IN ('urgent','high','medium','low')),
  -- Denormalized from the last message so "belum dibalas" is an indexed
  -- lookup instead of a correlated subquery per row.
  ADD COLUMN IF NOT EXISTS last_message_direction TEXT
    CHECK (last_message_direction IN ('inbound','outbound')),
  ADD COLUMN IF NOT EXISTS first_response_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS resolved_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS conversations_priority_idx
  ON conversations(org_id, priority, last_message_at DESC NULLS LAST);
CREATE INDEX IF NOT EXISTS conversations_unread_idx
  ON conversations(org_id) WHERE unread_count > 0;
CREATE INDEX IF NOT EXISTS conversations_awaiting_idx
  ON conversations(org_id) WHERE last_message_direction = 'inbound';

-- Labels apply to a contact as well as a conversation: the customer panel
-- labels the person, the inbox labels the thread.
CREATE TABLE IF NOT EXISTS contact_labels (
  contact_id UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  label_id   UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (contact_id, label_id)
);

-- ------------------------------------------------------- activity timeline
-- Append-only. Feeds the "Aktivitas" tab; distinct from audit_logs, which is
-- security/compliance and not shown to agents.
CREATE TABLE IF NOT EXISTS activities (
  id              BIGSERIAL PRIMARY KEY,
  org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
  contact_id      UUID REFERENCES contacts(id) ON DELETE CASCADE,
  actor_user_id   UUID REFERENCES users(id) ON DELETE SET NULL,
  kind            TEXT NOT NULL,
  summary         TEXT NOT NULL,
  metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS activities_conversation_idx
  ON activities(conversation_id, created_at DESC);
CREATE INDEX IF NOT EXISTS activities_contact_idx
  ON activities(contact_id, created_at DESC);

-- ------------------------------------------------------------------ deals
CREATE TABLE IF NOT EXISTS deals (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  contact_id  UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  pipeline_id UUID REFERENCES pipelines(id) ON DELETE SET NULL,
  stage_id    UUID REFERENCES pipeline_stages(id) ON DELETE SET NULL,
  title       TEXT NOT NULL,
  value_idr   BIGINT NOT NULL DEFAULT 0,
  status      TEXT NOT NULL DEFAULT 'open'
              CHECK (status IN ('open','won','lost')),
  owner_id    UUID REFERENCES users(id) ON DELETE SET NULL,
  expected_close_at DATE,
  lost_reason TEXT,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS deals_contact_idx ON deals(contact_id, created_at DESC);
CREATE INDEX IF NOT EXISTS deals_org_status_idx ON deals(org_id, status);

-- ------------------------------------------------------- Meta lead events
-- Click-to-WhatsApp / Meta CRM milestone reporting. Rows are kept whether or
-- not the upstream call succeeded, so the panel can show what was reported.
CREATE TABLE IF NOT EXISTS meta_lead_events (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  contact_id      UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  conversation_id UUID REFERENCES conversations(id) ON DELETE SET NULL,
  wa_account_id   UUID REFERENCES whatsapp_accounts(id) ON DELETE SET NULL,
  event_name      TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending','sent','failed')),
  payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
  provider_ref    TEXT,
  error           TEXT,
  sent_by         UUID REFERENCES users(id) ON DELETE SET NULL,
  sent_at         TIMESTAMPTZ,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS meta_lead_events_contact_idx
  ON meta_lead_events(contact_id, created_at DESC);
-- One successful report per milestone per contact; retries after a failure
-- are still allowed.
CREATE UNIQUE INDEX IF NOT EXISTS meta_lead_events_once_idx
  ON meta_lead_events(contact_id, event_name) WHERE status = 'sent';

-- Backfill the denormalized direction for any existing conversations.
UPDATE conversations c
SET last_message_direction = m.direction
FROM (
  SELECT DISTINCT ON (conversation_id) conversation_id, direction
  FROM messages ORDER BY conversation_id, created_at DESC
) m
WHERE m.conversation_id = c.id AND c.last_message_direction IS NULL;

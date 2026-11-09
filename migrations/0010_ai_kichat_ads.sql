-- 0010_ai_kichat_ads.sql — AI agent execution, Kichat browser sessions,
-- and Click-to-WhatsApp ad attribution.

-- ------------------------------------------------------------- AI agents
ALTER TABLE ai_agents
  ALTER COLUMN model SET DEFAULT 'claude-opus-5-5',
  ADD COLUMN IF NOT EXISTS max_turns INT NOT NULL DEFAULT 6,
  -- How much history to send. Long WhatsApp threads are mostly noise.
  ADD COLUMN IF NOT EXISTS context_messages INT NOT NULL DEFAULT 12,
  ADD COLUMN IF NOT EXISTS reply_delay_secs INT NOT NULL DEFAULT 2,
  ADD COLUMN IF NOT EXISTS fallback_message TEXT NOT NULL
    DEFAULT 'Mohon tunggu sebentar, tim kami akan segera membantu.';

-- One agent per number keeps routing unambiguous.
CREATE UNIQUE INDEX IF NOT EXISTS ai_agents_account_uidx
  ON ai_agents(wa_account_id) WHERE wa_account_id IS NOT NULL;

-- Audit of what the agent actually did, so a bad reply can be traced to the
-- exact model, prompt and token spend that produced it.
CREATE TABLE IF NOT EXISTS ai_runs (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  agent_id        UUID REFERENCES ai_agents(id) ON DELETE SET NULL,
  conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
  trigger_message_id UUID REFERENCES messages(id) ON DELETE SET NULL,
  reply_message_id   UUID REFERENCES messages(id) ON DELETE SET NULL,
  model           TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'ok'
                  CHECK (status IN ('ok','handoff','refused','skipped','failed')),
  intent          TEXT,
  handoff_reason  TEXT,
  input_tokens    INT,
  output_tokens   INT,
  latency_ms      INT,
  error           TEXT,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ai_runs_org_idx ON ai_runs(org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS ai_runs_conversation_idx ON ai_runs(conversation_id, created_at DESC);

-- ---------------------------------------------------------------- Kichat
-- A private browser chat room. The customer opens a link and keeps talking
-- without WhatsApp, which also sidesteps Meta's 24-hour window; the team
-- still answers from the same inbox.
CREATE TABLE IF NOT EXISTS kichat_sessions (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  contact_id      UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  -- Only the hash is stored; the raw token lives in the invite link.
  token_hash      TEXT NOT NULL UNIQUE,
  status          TEXT NOT NULL DEFAULT 'invited'
                  CHECK (status IN ('invited','opened','closed','expired')),
  opened_at       TIMESTAMPTZ,
  last_seen_at    TIMESTAMPTZ,
  expires_at      TIMESTAMPTZ NOT NULL,
  created_by      UUID REFERENCES users(id) ON DELETE SET NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS kichat_conversation_idx
  ON kichat_sessions(conversation_id, created_at DESC);

-- Messages typed in the browser land in the same thread as WhatsApp ones.
ALTER TABLE messages
  ADD COLUMN IF NOT EXISTS channel TEXT NOT NULL DEFAULT 'whatsapp'
    CHECK (channel IN ('whatsapp','kichat'));

ALTER TABLE conversations
  ADD COLUMN IF NOT EXISTS kichat_active BOOLEAN NOT NULL DEFAULT false;

-- 'ai_agent' and 'kichat' are both valid message sources now.
ALTER TABLE messages DROP CONSTRAINT IF EXISTS messages_source_check;
ALTER TABLE messages
  ADD CONSTRAINT messages_source_check
  CHECK (source IN ('api','dashboard','broadcast','ai_agent','system','kichat'));

-- ------------------------------------------------- Click-to-WhatsApp ads
-- Meta attaches a `referral` object to the first message from an ad click.
ALTER TABLE contacts
  ADD COLUMN IF NOT EXISTS source TEXT,
  ADD COLUMN IF NOT EXISTS referral JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE TABLE IF NOT EXISTS ad_referrals (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  contact_id      UUID NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  conversation_id UUID REFERENCES conversations(id) ON DELETE SET NULL,
  wa_account_id   UUID REFERENCES whatsapp_accounts(id) ON DELETE SET NULL,
  source_type     TEXT,                -- 'ad' | 'post'
  source_id       TEXT,                -- Meta ad or post id
  source_url      TEXT,
  headline        TEXT,
  body            TEXT,
  media_type      TEXT,
  ctwa_clid       TEXT,                -- click id, for conversion reporting
  raw             JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ad_referrals_org_idx ON ad_referrals(org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS ad_referrals_source_idx ON ad_referrals(org_id, source_id);

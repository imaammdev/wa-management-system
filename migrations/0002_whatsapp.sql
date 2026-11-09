-- 0002_whatsapp.sql — connected WhatsApp numbers, templates, media
CREATE TABLE whatsapp_accounts (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  customer_id UUID REFERENCES customers(id) ON DELETE CASCADE,
  provider    TEXT NOT NULL DEFAULT 'meta_cloud'
              CHECK (provider IN ('meta_cloud','mock')),
  label       TEXT NOT NULL,
  -- Meta identifiers
  waba_id             TEXT,
  phone_number_id     TEXT,
  display_phone_number TEXT,
  verified_name       TEXT,
  -- AES-256-GCM sealed system user access token (nonce prepended, hex)
  access_token_enc    TEXT,
  -- token Meta sends back on our inbound webhook, so we can verify subscription
  webhook_verify_token TEXT,
  status      TEXT NOT NULL DEFAULT 'pending'
              CHECK (status IN ('pending','connected','disconnected','error')),
  quality_rating TEXT,
  messaging_limit_tier TEXT,
  mode        TEXT NOT NULL DEFAULT 'api'
              CHECK (mode IN ('api','coexistence')),
  provider_meta JSONB NOT NULL DEFAULT '{}'::jsonb,
  last_error  TEXT,
  connected_at TIMESTAMPTZ,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX wa_accounts_phone_number_id_uidx
  ON whatsapp_accounts(phone_number_id) WHERE phone_number_id IS NOT NULL;
CREATE INDEX wa_accounts_org_idx ON whatsapp_accounts(org_id);
CREATE INDEX wa_accounts_customer_idx ON whatsapp_accounts(customer_id);

CREATE TABLE message_templates (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  wa_account_id UUID NOT NULL REFERENCES whatsapp_accounts(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  language      TEXT NOT NULL DEFAULT 'id',
  category      TEXT NOT NULL
                CHECK (category IN ('MARKETING','UTILITY','AUTHENTICATION')),
  components    JSONB NOT NULL DEFAULT '[]'::jsonb,
  status        TEXT NOT NULL DEFAULT 'PENDING'
                CHECK (status IN ('PENDING','APPROVED','REJECTED','PAUSED','DISABLED')),
  rejected_reason TEXT,
  provider_ref  TEXT,
  synced_at     TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (wa_account_id, name, language)
);
CREATE INDEX templates_org_idx ON message_templates(org_id);

CREATE TABLE media_objects (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  wa_account_id UUID REFERENCES whatsapp_accounts(id) ON DELETE SET NULL,
  provider_media_id TEXT,
  storage_key   TEXT NOT NULL,
  mime_type     TEXT NOT NULL,
  size_bytes    BIGINT NOT NULL DEFAULT 0,
  sha256        TEXT,
  filename      TEXT,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX media_org_idx ON media_objects(org_id);

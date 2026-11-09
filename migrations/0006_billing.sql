-- 0006_billing.sql — plans, subscriptions, IDR invoicing, quota metering
CREATE TABLE plans (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code        TEXT NOT NULL UNIQUE,
  name        TEXT NOT NULL,
  tagline     TEXT,
  price_idr_monthly BIGINT NOT NULL,
  price_idr_yearly  BIGINT NOT NULL,
  is_public   BOOLEAN NOT NULL DEFAULT true,
  is_popular  BOOLEAN NOT NULL DEFAULT false,
  position    INT NOT NULL DEFAULT 0,
  -- hard caps enforced by core::quota
  max_broadcasts_per_month BIGINT NOT NULL,
  max_messages_per_month   BIGINT NOT NULL,
  max_storage_bytes        BIGINT NOT NULL,
  max_members              INT NOT NULL,
  max_wa_accounts          INT NOT NULL,
  history_retention_days   INT NOT NULL,
  features    JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE addons (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code       TEXT NOT NULL UNIQUE,
  name       TEXT NOT NULL,
  price_idr_monthly BIGINT NOT NULL,
  unit       TEXT NOT NULL DEFAULT 'unit',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE subscriptions (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  plan_id    UUID NOT NULL REFERENCES plans(id),
  status     TEXT NOT NULL DEFAULT 'trialing'
             CHECK (status IN ('trialing','active','past_due','cancelled','expired')),
  interval   TEXT NOT NULL DEFAULT 'monthly'
             CHECK (interval IN ('monthly','yearly')),
  current_period_start TIMESTAMPTZ NOT NULL DEFAULT now(),
  current_period_end   TIMESTAMPTZ NOT NULL,
  trial_ends_at        TIMESTAMPTZ,
  cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
  cancelled_at         TIMESTAMPTZ,
  -- 'midtrans' | 'xendit' | 'manual'
  payment_provider     TEXT NOT NULL DEFAULT 'manual',
  provider_ref         TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX subscriptions_active_org_uidx ON subscriptions(org_id)
  WHERE status IN ('trialing','active','past_due');

CREATE TABLE subscription_addons (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
  addon_id        UUID NOT NULL REFERENCES addons(id),
  quantity        INT NOT NULL DEFAULT 1,
  UNIQUE (subscription_id, addon_id)
);

CREATE TABLE invoices (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  subscription_id UUID REFERENCES subscriptions(id) ON DELETE SET NULL,
  number     TEXT NOT NULL UNIQUE,
  status     TEXT NOT NULL DEFAULT 'open'
             CHECK (status IN ('draft','open','paid','void','uncollectible')),
  subtotal_idr BIGINT NOT NULL DEFAULT 0,
  tax_idr      BIGINT NOT NULL DEFAULT 0,   -- PPN 11%
  total_idr    BIGINT NOT NULL DEFAULT 0,
  line_items JSONB NOT NULL DEFAULT '[]'::jsonb,
  due_at     TIMESTAMPTZ,
  paid_at    TIMESTAMPTZ,
  payment_provider TEXT,
  provider_ref     TEXT,
  payment_url      TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX invoices_org_idx ON invoices(org_id, created_at DESC);

-- One row per org per metric per month. Incremented transactionally on send.
CREATE TABLE usage_counters (
  org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  period_month DATE NOT NULL,            -- first day of month
  metric       TEXT NOT NULL
               CHECK (metric IN ('messages_sent','messages_received',
                                 'broadcasts_sent','storage_bytes',
                                 'marketing_messages','utility_messages',
                                 'authentication_messages','service_messages')),
  value        BIGINT NOT NULL DEFAULT 0,
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (org_id, period_month, metric)
);

-- Meta's per-message rates, used by the public pricing calculator.
CREATE TABLE meta_rates (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  country    TEXT NOT NULL DEFAULT 'ID',
  category   TEXT NOT NULL
             CHECK (category IN ('marketing','utility','authentication','service')),
  rate_idr   NUMERIC(12,2) NOT NULL,
  free_tier_per_number_per_month INT NOT NULL DEFAULT 0,
  effective_from DATE NOT NULL,
  UNIQUE (country, category, effective_from)
);

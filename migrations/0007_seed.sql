-- 0007_seed.sql — plans, add-ons and Meta rates (IDR, Indonesia)
INSERT INTO plans (code, name, tagline, price_idr_monthly, price_idr_yearly,
  is_popular, position, max_broadcasts_per_month, max_messages_per_month,
  max_storage_bytes, max_members, max_wa_accounts, history_retention_days, features)
VALUES
  ('starter','Starter','Untuk mencoba platform',
    25000, 250000, true, 1,
    2000, 200000, 524288000, 5, 1, 90,
    '{"multi_tenant":true,"api":true,"webhooks":true,"shared_inbox":true,"ai_agent":false}'),
  ('pro','Pro','Untuk bisnis berkembang',
    149000, 1490000, false, 2,
    10000, 500000, 2147483648, 10, 7, 365,
    '{"multi_tenant":true,"api":true,"webhooks":true,"shared_inbox":true,"ai_agent":true}'),
  ('business','Business','Untuk agensi & platform multi-tenant',
    199000, 1990000, false, 3,
    50000, 1000000, 5368709120, 20, 10, 365,
    '{"multi_tenant":true,"api":true,"webhooks":true,"shared_inbox":true,"ai_agent":true,"priority_support":true}')
ON CONFLICT (code) DO NOTHING;

INSERT INTO addons (code, name, price_idr_monthly, unit) VALUES
  ('extra_wa_number','Extra WhatsApp Number', 49000, 'nomor'),
  ('extra_member','Extra Member', 15000, 'member')
ON CONFLICT (code) DO NOTHING;

-- Meta Indonesia rates. 'service' gets 1000 free per number per month from 2026-10-01.
INSERT INTO meta_rates (country, category, rate_idr, free_tier_per_number_per_month, effective_from) VALUES
  ('ID','marketing',       586.33, 0,    DATE '2026-10-01'),
  ('ID','utility',         356.65, 0,    DATE '2026-10-01'),
  ('ID','authentication',  356.65, 0,    DATE '2026-10-01'),
  ('ID','service',         356.65, 1000, DATE '2026-10-01')
ON CONFLICT (country, category, effective_from) DO NOTHING;

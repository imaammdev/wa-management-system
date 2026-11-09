-- 0009_settings_analytics.sql — org settings, auto-assignment, and the
-- indexes the analytics aggregates rely on.

ALTER TABLE organizations
  -- { "business_hours": {...}, "auto_assign": "round_robin"|"off",
  --   "auto_close_after_hours": 48, "away_message": "..." }
  ADD COLUMN IF NOT EXISTS settings JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Analytics scans messages by org over a date window, grouped by day and by
-- direction/status. Without this every chart is a full table scan.
CREATE INDEX IF NOT EXISTS messages_org_day_idx
  ON messages (org_id, created_at DESC, direction);
CREATE INDEX IF NOT EXISTS messages_org_status_idx
  ON messages (org_id, status) WHERE direction = 'outbound';
CREATE INDEX IF NOT EXISTS conversations_resolved_idx
  ON conversations (org_id, resolved_at) WHERE resolved_at IS NOT NULL;

-- Response-time reporting needs both ends of the first exchange.
CREATE INDEX IF NOT EXISTS conversations_first_response_idx
  ON conversations (org_id, first_response_at) WHERE first_response_at IS NOT NULL;

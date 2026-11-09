/**
 * Thin fetch wrapper around the Rust API.
 *
 * The dashboard authenticates with the session cookie, and sends X-Org-Id so
 * the backend knows which organization the user is currently acting in.
 */

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public details?: unknown,
  ) {
    super(message);
  }
}

const ORG_KEY = "mms.org_id";

export function currentOrgId(): string | null {
  try {
    return localStorage.getItem(ORG_KEY);
  } catch {
    return null;
  }
}

export function setCurrentOrgId(id: string | null) {
  try {
    if (id) localStorage.setItem(ORG_KEY, id);
    else localStorage.removeItem(ORG_KEY);
  } catch {
    /* private mode — fall back to per-request default org */
  }
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["content-type"] = "application/json";
  const org = currentOrgId();
  if (org) headers["x-org-id"] = org;

  const res = await fetch(path, {
    method,
    headers,
    credentials: "same-origin",
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (res.status === 204) return undefined as T;

  const text = await res.text();
  const parsed = text ? JSON.parse(text) : {};

  if (!res.ok) {
    const e = parsed?.error ?? {};
    throw new ApiError(
      res.status,
      e.code ?? "unknown",
      e.message ?? `HTTP ${res.status}`,
      e.details,
    );
  }
  return parsed as T;
}

export const api = {
  get: <T>(p: string) => request<T>("GET", p),
  post: <T>(p: string, b?: unknown) => request<T>("POST", p, b),
  patch: <T>(p: string, b?: unknown) => request<T>("PATCH", p, b),
  del: <T>(p: string) => request<T>("DELETE", p),
};

// ----------------------------------------------------------------- types

export type Org = { id: string; name: string; slug: string; role: string };
export type User = { id: string; name: string; email: string };

export type Plan = {
  id: string;
  code: string;
  name: string;
  tagline: string | null;
  price_idr_monthly: number;
  price_idr_yearly: number;
  is_popular: boolean;
  max_broadcasts_per_month: number;
  max_messages_per_month: number;
  max_storage_bytes: number;
  max_members: number;
  max_wa_accounts: number;
  history_retention_days: number;
};

export type WaAccount = {
  id: string;
  label: string;
  provider: string;
  status: string;
  display_phone_number: string | null;
  verified_name: string | null;
  quality_rating: string | null;
  phone_number_id: string | null;
  waba_id: string | null;
};

export type Label = { id: string; name: string; color: string };

export type Contact = {
  id: string;
  wa_id: string;
  name: string | null;
  profile_name: string | null;
  email: string | null;
  attributes: Record<string, unknown>;
  opt_in_status: string;
  last_seen_at: string | null;
  created_at: string;
};

export type Conversation = {
  id: string;
  status: string;
  priority: "urgent" | "high" | "medium" | "low";
  assigned_to: string | null;
  assignee_name: string | null;
  wa_account_id: string;
  contact: { wa_id: string; name: string | null };
  labels: Label[];
  last_message_at: string | null;
  last_message_preview: string | null;
  last_message_direction: "inbound" | "outbound" | null;
  awaiting_reply: boolean;
  unread_count: number;
  service_window_open: boolean;
};

export type InboxCounts = {
  total: number;
  unread: number;
  awaiting: number;
  open: number;
  pending: number;
  held: number;
  closed: number;
  urgent: number;
  mine: number;
  unassigned: number;
};

export type Member = { id: string; name: string; email: string; role: string };

export type Activity = {
  id: number;
  kind: string;
  summary: string;
  actor_name: string | null;
  created_at: string;
};

export type Note = {
  id: string;
  body: string;
  author_name: string | null;
  created_at: string;
};

export type Deal = {
  id: string;
  title: string;
  value_idr: number;
  status: "open" | "won" | "lost";
  stage_name: string | null;
  expected_close_at: string | null;
  created_at: string;
};

export type MetaEvent = {
  id: string;
  event_name: string;
  status: "pending" | "sent" | "failed";
  error: string | null;
  sent_at: string | null;
  created_at: string;
};

export type ContactPanel = {
  id: string;
  wa_id: string;
  name: string | null;
  profile_name: string | null;
  email: string | null;
  opt_in_status: string;
  last_seen_at: string | null;
  created_at: string;
  labels: Label[];
  deals: Deal[];
  meta_events: MetaEvent[];
};

export type CannedResponse = { id: string; shortcut: string; body: string };

export type Message = {
  id: string;
  direction: "inbound" | "outbound";
  msg_type: string;
  body: string | null;
  status: string;
  source: string;
  error_detail: string | null;
  created_at: string;
};

export type ApiKeyRow = {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  last_used_at: string | null;
  revoked_at: string | null;
  created_at: string;
};

export type WebhookEndpointRow = {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  description: string | null;
};

export type DeliveryRow = {
  id: string;
  event_type: string;
  status: string;
  attempt: number;
  max_attempts: number;
  response_status: number | null;
  last_error: string | null;
  created_at: string;
};

export type Usage = {
  plan_code: string;
  messages_sent: number;
  messages_limit: number;
  broadcasts_sent: number;
  broadcasts_limit: number;
  wa_accounts_used: number;
  wa_accounts_limit: number;
  members_used: number;
  members_limit: number;
};

export const rupiah = (n: number) =>
  new Intl.NumberFormat("id-ID", {
    style: "currency",
    currency: "IDR",
    maximumFractionDigits: 0,
  }).format(n);

export const waktu = (iso: string | null) =>
  iso
    ? new Intl.DateTimeFormat("id-ID", {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(iso))
    : "—";


/** "9 jam", "Sel", "21 Agu" — the inbox's compact relative time. */
export function waktuSingkat(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  const mins = Math.floor((Date.now() - d.getTime()) / 60000);
  if (mins < 1) return "baru";
  if (mins < 60) return `${mins} mnt`;
  if (mins < 60 * 24) return `${Math.floor(mins / 60)} jam`;
  if (mins < 60 * 24 * 7)
    return new Intl.DateTimeFormat("id-ID", { weekday: "short" }).format(d);
  return new Intl.DateTimeFormat("id-ID", { day: "numeric", month: "short" }).format(d);
}

export const jam = (iso: string) =>
  new Intl.DateTimeFormat("id-ID", { hour: "2-digit", minute: "2-digit" }).format(
    new Date(iso),
  );

export const hari = (iso: string) =>
  new Intl.DateTimeFormat("id-ID", {
    day: "numeric",
    month: "short",
    year: "numeric",
  })
    .format(new Date(iso))
    .toUpperCase();

export const inisial = (name: string | null, fallback: string) =>
  (name ?? fallback)
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0] ?? "")
    .join("")
    .toUpperCase() || "?";

export const PRIORITAS: Record<string, { label: string; color: string }> = {
  urgent: { label: "Mendesak", color: "#f2555a" },
  high: { label: "Tinggi", color: "#f5a524" },
  medium: { label: "Sedang", color: "#4f8cff" },
  low: { label: "Rendah", color: "#6b779b" },
};

export const STATUS_ID: Record<string, string> = {
  open: "Terbuka",
  pending: "Menunggu",
  held: "Ditahan",
  snoozed: "Ditunda",
  closed: "Selesai",
};

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

export type Conversation = {
  id: string;
  status: string;
  assigned_to: string | null;
  wa_account_id: string;
  contact: { wa_id: string; name: string | null };
  last_message_at: string | null;
  last_message_preview: string | null;
  unread_count: number;
  service_window_open: boolean;
};

export type Message = {
  id: string;
  direction: "inbound" | "outbound";
  msg_type: string;
  body: string | null;
  status: string;
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

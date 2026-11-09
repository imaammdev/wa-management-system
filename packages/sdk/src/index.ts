/**
 * @mms/sdk — typed client for the MMS WhatsApp Business API.
 *
 *   const mms = new Mms({ apiKey: process.env.MMS_API_KEY! });
 *   await mms.messages.sendText({
 *     waAccountId: "…", to: "628123456789", body: "Halo!",
 *   });
 */

export type MmsOptions = {
  apiKey: string;
  /** Defaults to the hosted API. Point at http://localhost:8080 in dev. */
  baseUrl?: string;
  /** Per-request timeout in ms. Default 30s. */
  timeoutMs?: number;
  fetch?: typeof globalThis.fetch;
};

export class MmsError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public details?: unknown,
  ) {
    super(message);
    this.name = "MmsError";
  }

  /** Transient conditions worth retrying with backoff. */
  get retryable(): boolean {
    return this.status === 429 || this.status >= 500;
  }
}

export type SendResult = {
  messaging_product: "whatsapp";
  id: string;
  wa_message_id: string | null;
  conversation_id: string;
  status: string;
};

export type Contact = {
  id: string;
  wa_id: string;
  name: string | null;
  profile_name: string | null;
  opt_in_status: "unknown" | "opted_in" | "opted_out";
};

export type Template = {
  id: string;
  name: string;
  language: string;
  category: "MARKETING" | "UTILITY" | "AUTHENTICATION";
  status: string;
};

export type Number_ = {
  id: string;
  label: string;
  display_phone_number: string | null;
  status: string;
  quality_rating: string | null;
};

type Paged<T> = { data: T[]; has_more?: boolean };

export class Mms {
  private baseUrl: string;
  private apiKey: string;
  private timeoutMs: number;
  private fetchImpl: typeof globalThis.fetch;

  constructor(opts: MmsOptions) {
    if (!opts.apiKey) throw new Error("apiKey is required");
    this.apiKey = opts.apiKey;
    this.baseUrl = (opts.baseUrl ?? "https://api.mms.id").replace(/\/+$/, "");
    this.timeoutMs = opts.timeoutMs ?? 30_000;
    this.fetchImpl = opts.fetch ?? globalThis.fetch;
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    idempotencyKey?: string,
  ): Promise<T> {
    const headers: Record<string, string> = {
      authorization: `Bearer ${this.apiKey}`,
    };
    if (body !== undefined) headers["content-type"] = "application/json";
    if (idempotencyKey) headers["idempotency-key"] = idempotencyKey;

    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), this.timeoutMs);

    try {
      const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: ctrl.signal,
      });

      const text = await res.text();
      const parsed = text ? JSON.parse(text) : {};

      if (!res.ok) {
        const e = parsed?.error ?? {};
        throw new MmsError(
          res.status,
          e.code ?? "unknown",
          e.message ?? `HTTP ${res.status}`,
          e.details,
        );
      }
      return parsed as T;
    } finally {
      clearTimeout(timer);
    }
  }

  readonly messages = {
    /** Free-form text. Only valid inside the 24h customer service window. */
    sendText: (p: {
      waAccountId: string;
      to: string;
      body: string;
      previewUrl?: boolean;
      replyTo?: string;
      idempotencyKey?: string;
    }) =>
      this.request<SendResult>(
        "POST",
        "/v1/messages",
        {
          wa_account_id: p.waAccountId,
          to: p.to,
          type: "text",
          body: p.body,
          preview_url: p.previewUrl ?? false,
          reply_to: p.replyTo,
        },
        p.idempotencyKey,
      ),

    /** An approved template — the only thing sendable outside the 24h window. */
    sendTemplate: (p: {
      waAccountId: string;
      to: string;
      name: string;
      language?: string;
      components?: unknown;
      idempotencyKey?: string;
    }) =>
      this.request<SendResult>(
        "POST",
        "/v1/messages",
        {
          wa_account_id: p.waAccountId,
          to: p.to,
          type: "template",
          name: p.name,
          language: p.language ?? "id",
          components: p.components ?? [],
        },
        p.idempotencyKey,
      ),

    sendImage: (p: {
      waAccountId: string;
      to: string;
      link: string;
      caption?: string;
      idempotencyKey?: string;
    }) =>
      this.request<SendResult>(
        "POST",
        "/v1/messages",
        {
          wa_account_id: p.waAccountId,
          to: p.to,
          type: "image",
          link: p.link,
          caption: p.caption,
        },
        p.idempotencyKey,
      ),

    get: (id: string) => this.request<{ data: unknown }>("GET", `/v1/messages/${id}`),
  };

  readonly contacts = {
    list: (p: { search?: string; limit?: number; offset?: number } = {}) => {
      const q = new URLSearchParams();
      if (p.search) q.set("search", p.search);
      if (p.limit) q.set("limit", String(p.limit));
      if (p.offset) q.set("offset", String(p.offset));
      const qs = q.toString();
      return this.request<Paged<Contact>>("GET", `/v1/contacts${qs ? `?${qs}` : ""}`);
    },

    upsert: (p: { waId: string; name?: string }) =>
      this.request<{ data: Contact }>("POST", "/v1/contacts", {
        wa_id: p.waId,
        name: p.name,
      }),
  };

  readonly templates = {
    list: () => this.request<Paged<Template>>("GET", "/v1/templates"),
  };

  readonly numbers = {
    list: () => this.request<Paged<Number_>>("GET", "/v1/numbers"),
  };
}

/**
 * Verifies an inbound webhook from MMS.
 *
 * Compare against the RAW request body — not a re-serialized object, or the
 * bytes won't match. Uses WebCrypto, so it runs on Node 18+, Bun, Deno,
 * Cloudflare Workers and Vercel Edge alike.
 */
export async function verifyWebhookSignature(
  secret: string,
  rawBody: string | Uint8Array,
  signatureHeader: string,
): Promise<boolean> {
  const enc = new TextEncoder();
  const bytes = typeof rawBody === "string" ? enc.encode(rawBody) : rawBody;

  const key = await crypto.subtle.importKey(
    "raw",
    enc.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign("HMAC", key, bytes as BufferSource);
  const expected =
    "sha256=" +
    Array.from(new Uint8Array(mac))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");

  // Constant-time compare.
  if (expected.length !== signatureHeader.length) return false;
  let diff = 0;
  for (let i = 0; i < expected.length; i++) {
    diff |= expected.charCodeAt(i) ^ signatureHeader.charCodeAt(i);
  }
  return diff === 0;
}

export type WebhookEvent =
  | {
      id: string;
      type: "message.received";
      created_at: string;
      data: {
        message: {
          id: string;
          wa_message_id: string;
          conversation_id: string;
          type: string;
          text: string | null;
          from: string;
          timestamp: string;
        };
        contact: { wa_id: string; profile_name: string | null };
        wa_account_id: string;
      };
    }
  | {
      id: string;
      type: "message.status";
      created_at: string;
      data: {
        message_id: string;
        wa_message_id: string;
        status: "sent" | "delivered" | "read" | "failed";
        pricing_category: string | null;
        error: { code: string; title: string | null } | null;
        wa_account_id: string;
      };
    };

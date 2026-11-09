import { useCallback, useEffect, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { api, jam } from "../api";

type Msg = {
  id: string;
  direction: "inbound" | "outbound";
  body: string | null;
  channel: string;
  created_at: string;
};

type Room = {
  business: string;
  customer: string | null;
  expires_at: string;
  messages: Msg[];
};

/**
 * The customer-facing chat room. No account, no app — the link token is the
 * whole credential, so this page shows one conversation and nothing else.
 */
export function Kichat() {
  const { token = "" } = useParams();
  const [room, setRoom] = useState<Room | null>(null);
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [err, setErr] = useState("");
  const [gone, setGone] = useState("");
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void (async () => {
      try {
        const r = await api.get<Room>(`/kichat/${token}/room`);
        setRoom(r);
        setMsgs(r.messages);
      } catch (e) {
        // A closed or expired link is a normal end state, not an error.
        setGone(e instanceof Error ? e.message : "Tautan tidak berlaku.");
      }
    })();
  }, [token]);

  const poll = useCallback(async () => {
    if (!room) return;
    const after = msgs.length ? msgs[msgs.length - 1].created_at : undefined;
    try {
      const r = await api.get<{ messages: Msg[] }>(
        `/kichat/${token}/messages${after ? `?after=${encodeURIComponent(after)}` : ""}`,
      );
      if (r.messages.length) setMsgs((m) => [...m, ...r.messages]);
    } catch {
      /* transient — the next tick retries */
    }
  }, [room, msgs, token]);

  useEffect(() => {
    if (!room) return;
    const t = setInterval(() => void poll(), 4000);
    return () => clearInterval(t);
  }, [room, poll]);

  useEffect(() => {
    bodyRef.current?.scrollTo({ top: bodyRef.current.scrollHeight });
  }, [msgs]);

  const send = async (e: React.FormEvent) => {
    e.preventDefault();
    const text = draft.trim();
    if (!text || sending) return;
    setSending(true);
    setErr("");
    try {
      await api.post(`/kichat/${token}/messages`, { body: text });
      setDraft("");
      await poll();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal mengirim pesan.");
    } finally {
      setSending(false);
    }
  };

  if (gone) {
    return (
      <div className="center-page">
        <div className="card auth-card" style={{ textAlign: "center" }}>
          <h2>Ruang chat tidak tersedia</h2>
          <p className="muted">{gone}</p>
          <p className="hint">Silakan hubungi kami kembali lewat WhatsApp.</p>
        </div>
      </div>
    );
  }

  if (!room) {
    return (
      <div className="center-page">
        <p className="muted">Memuat ruang chat…</p>
      </div>
    );
  }

  return (
    <div className="center-page" style={{ padding: "1rem" }}>
      <div
        className="thread"
        style={{ width: "100%", maxWidth: "34rem", height: "min(86vh, 44rem)" }}
      >
        <div className="thread-head">
          <strong>{room.business}</strong>
          <div className="faint" style={{ fontSize: "0.78rem" }}>
            Ruang chat privat{room.customer ? ` untuk ${room.customer}` : ""} · Kichat
          </div>
        </div>

        <div className="thread-body" ref={bodyRef}>
          {msgs.length === 0 && (
            <p className="faint" style={{ margin: "auto", textAlign: "center" }}>
              Belum ada pesan. Tulis pertanyaan Anda di bawah.
            </p>
          )}
          {msgs.map((m) => (
            <div
              key={m.id}
              /* Mirrored: the customer's own messages sit on the right. */
              className={"bubble " + (m.direction === "inbound" ? "bubble-out" : "bubble-in")}
              style={{ alignSelf: m.direction === "inbound" ? "flex-end" : "flex-start" }}
            >
              {m.body}
              <div className="bubble-meta">{jam(m.created_at)}</div>
            </div>
          ))}
        </div>

        {err && (
          <div className="alert" style={{ margin: "0.5rem", marginBottom: 0 }}>
            {err}
          </div>
        )}

        <form className="composer" onSubmit={send}>
          <input
            className="input"
            placeholder="Tulis pesan…"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            maxLength={4000}
          />
          <button className="btn btn-primary" disabled={sending || !draft.trim()}>
            {sending ? "…" : "Kirim"}
          </button>
        </form>
      </div>
    </div>
  );
}

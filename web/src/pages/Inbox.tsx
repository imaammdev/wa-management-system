import { useEffect, useRef, useState } from "react";
import { api, waktu, type Conversation, type Message } from "../api";
import { Empty, Err, PageHead, StatusBadge } from "../components/bits";

const FILTERS = [
  { key: "open", label: "Terbuka" },
  { key: "pending", label: "Menunggu" },
  { key: "closed", label: "Selesai" },
  { key: "", label: "Semua" },
];

export function Inbox() {
  const [convs, setConvs] = useState<Conversation[]>([]);
  const [filter, setFilter] = useState("open");
  const [active, setActive] = useState<Conversation | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState("");
  const [err, setErr] = useState("");
  const [sending, setSending] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  const loadConvs = async () => {
    try {
      const q = filter ? `?status=${filter}` : "";
      const r = await api.get<{ data: Conversation[] }>(`/api/inbox/conversations${q}`);
      setConvs(r.data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat percakapan.");
    }
  };

  const loadMessages = async (id: string) => {
    const r = await api.get<{ data: Message[] }>(
      `/api/inbox/conversations/${id}/messages?limit=100`,
    );
    // API returns newest first; render oldest at the top like a chat.
    setMessages(r.data.slice().reverse());
    await api.post(`/api/inbox/conversations/${id}/read`).catch(() => {});
  };

  useEffect(() => {
    void loadConvs();
  }, [filter]);

  // Light polling keeps the inbox live without a websocket for now.
  useEffect(() => {
    const t = setInterval(() => {
      void loadConvs();
      if (active) void loadMessages(active.id).catch(() => {});
    }, 5000);
    return () => clearInterval(t);
  }, [active, filter]);

  useEffect(() => {
    bodyRef.current?.scrollTo({ top: bodyRef.current.scrollHeight });
  }, [messages]);

  const open = async (c: Conversation) => {
    setActive(c);
    setMessages([]);
    try {
      await loadMessages(c.id);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat pesan.");
    }
  };

  const send = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!active || !draft.trim()) return;
    setSending(true);
    setErr("");
    try {
      await api.post(`/api/inbox/conversations/${active.id}/messages`, {
        type: "text",
        body: draft,
      });
      setDraft("");
      await loadMessages(active.id);
      await loadConvs();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal mengirim.");
    } finally {
      setSending(false);
    }
  };

  const setStatus = async (status: string) => {
    if (!active) return;
    await api.post(`/api/inbox/conversations/${active.id}/status`, { status });
    await loadConvs();
  };

  return (
    <>
      <PageHead
        title="Inbox"
        sub="Satu nomor untuk seluruh tim. Balasan bebas hanya bisa dalam 24 jam setelah pelanggan menulis."
        action={
          <div className="toggle">
            {FILTERS.map((f) => (
              <button
                key={f.key}
                className={filter === f.key ? "on" : ""}
                onClick={() => setFilter(f.key)}
              >
                {f.label}
              </button>
            ))}
          </div>
        }
      />
      <Err>{err}</Err>

      {convs.length === 0 && !active ? (
        <Empty>
          Belum ada percakapan. Kirim pesan lewat API, atau pakai tombol
          &ldquo;Simulasi pesan masuk&rdquo; pada nomor bertipe <code>mock</code>.
        </Empty>
      ) : (
        <div className="inbox">
          <div className="conv-list">
            {convs.map((c) => (
              <div
                key={c.id}
                className={"conv-item" + (active?.id === c.id ? " active" : "")}
                onClick={() => void open(c)}
              >
                <div className="between">
                  <span className="conv-name">
                    {c.contact.name ?? c.contact.wa_id}
                  </span>
                  {c.unread_count > 0 && (
                    <span className="badge badge-ok">{c.unread_count}</span>
                  )}
                </div>
                <div className="conv-preview">{c.last_message_preview ?? "—"}</div>
                <div className="faint" style={{ fontSize: "0.72rem", marginTop: "0.2rem" }}>
                  {waktu(c.last_message_at)}
                  {!c.service_window_open && " · jendela 24j tertutup"}
                </div>
              </div>
            ))}
          </div>

          <div className="thread">
            {!active ? (
              <div style={{ margin: "auto", color: "var(--text-faint)" }}>
                Pilih percakapan.
              </div>
            ) : (
              <>
                <div className="thread-head between wrap">
                  <div>
                    <strong>{active.contact.name ?? active.contact.wa_id}</strong>
                    <div className="faint mono" style={{ fontSize: "0.78rem" }}>
                      {active.contact.wa_id}
                    </div>
                  </div>
                  <div className="row">
                    <StatusBadge status={active.status} />
                    <button className="btn btn-sm" onClick={() => void setStatus("closed")}>
                      Tandai selesai
                    </button>
                  </div>
                </div>

                <div className="thread-body" ref={bodyRef}>
                  {messages.map((m) => (
                    <div
                      key={m.id}
                      className={
                        "bubble " + (m.direction === "inbound" ? "bubble-in" : "bubble-out")
                      }
                    >
                      {m.body ?? <em className="faint">[{m.msg_type}]</em>}
                      <div className="bubble-meta">
                        {waktu(m.created_at)}
                        {m.direction === "outbound" && ` · ${m.status}`}
                        {m.error_detail && ` · ${m.error_detail}`}
                      </div>
                    </div>
                  ))}
                  {messages.length === 0 && (
                    <p className="faint" style={{ margin: "auto" }}>
                      Belum ada pesan.
                    </p>
                  )}
                </div>

                {active.service_window_open ? (
                  <form className="composer" onSubmit={send}>
                    <input
                      className="input"
                      placeholder="Tulis balasan…"
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                    />
                    <button className="btn btn-primary" disabled={sending || !draft.trim()}>
                      {sending ? "…" : "Kirim"}
                    </button>
                  </form>
                ) : (
                  <div className="composer">
                    <p className="hint" style={{ margin: 0 }}>
                      Jendela layanan 24 jam sudah tertutup. Kirim template yang
                      sudah disetujui lewat halaman Broadcast atau API.
                    </p>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </>
  );
}

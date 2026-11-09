import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api";
import type { CannedResponse, Conversation, Member, Message } from "../../api";
import { hari, jam, PRIORITAS, STATUS_ID } from "../../api";

/** Groups messages under date separators, oldest first. */
function withDays(msgs: Message[]) {
  const out: Array<{ day: string } | { msg: Message }> = [];
  let last = "";
  for (const m of msgs) {
    const d = hari(m.created_at);
    if (d !== last) {
      out.push({ day: d });
      last = d;
    }
    out.push({ msg: m });
  }
  return out;
}

function Ticks({ status }: { status: string }) {
  if (status === "failed") return <span style={{ color: "var(--danger)" }}>gagal</span>;
  if (status === "queued") return <span className="ticks">·</span>;
  if (status === "sent") return <span className="ticks">✓</span>;
  if (status === "delivered") return <span className="ticks">✓✓</span>;
  if (status === "read") return <span className="ticks read">✓✓</span>;
  return null;
}

export function Thread({
  conv,
  messages,
  members,
  canned,
  onReply,
  onStatus,
  onAssign,
  onPriority,
  onTogglePanel,
}: {
  conv: Conversation;
  messages: Message[];
  members: Member[];
  canned: CannedResponse[];
  onReply: (text: string) => Promise<void>;
  onStatus: (s: string) => void;
  onAssign: (userId: string | null) => void;
  onPriority: (p: string) => void;
  onTogglePanel: () => void;
}) {
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [pick, setPick] = useState(0);
  const [kichatUrl, setKichatUrl] = useState<string | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);

  // Kichat is our own transport, so a room keeps the thread answerable after
  // Meta's 24-hour window closes.
  const inviteKichat = async () => {
    const r = await api.post<{ url: string }>(
      `/api/kichat/conversations/${conv.id}/invite`,
      {},
    );
    setKichatUrl(r.url);
  };

  // A leading "/" opens the canned-response picker, filtered as you type.
  const shortcutQuery = draft.startsWith("/") ? draft.slice(1).toLowerCase() : null;
  const matches = useMemo(
    () =>
      shortcutQuery === null
        ? []
        : canned.filter((c) => c.shortcut.toLowerCase().startsWith(shortcutQuery)),
    [shortcutQuery, canned],
  );

  useEffect(() => setPick(0), [shortcutQuery]);
  useEffect(() => {
    bodyRef.current?.scrollTo({ top: bodyRef.current.scrollHeight });
  }, [messages, conv.id]);

  const send = async () => {
    const text = draft.trim();
    if (!text || sending) return;
    setSending(true);
    try {
      await onReply(text);
      setDraft("");
    } finally {
      setSending(false);
    }
  };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (matches.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setPick((p) => Math.min(p + 1, matches.length - 1));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setPick((p) => Math.max(p - 1, 0));
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && matches[pick])) {
        e.preventDefault();
        setDraft(matches[pick].body);
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  };

  const items = withDays(messages);

  return (
    <div className="thread-col">
      <div className="thread-bar">
        <span className="avatar">
          {(conv.contact.name ?? conv.contact.wa_id).slice(0, 2).toUpperCase()}
        </span>
        <div style={{ minWidth: 0 }}>
          <strong>{conv.contact.name ?? conv.contact.wa_id}</strong>
          <div className="faint mono" style={{ fontSize: "0.76rem" }}>
            +{conv.contact.wa_id}
          </div>
        </div>

        <div className="row" style={{ marginLeft: "auto", flexWrap: "wrap" }}>
          <select
            className="select"
            style={{ width: "auto", fontSize: "0.8rem", padding: "0.3rem 0.5rem" }}
            value={conv.priority}
            onChange={(e) => onPriority(e.target.value)}
            title="Prioritas"
          >
            {Object.entries(PRIORITAS).map(([k, p]) => (
              <option key={k} value={k}>
                {p.label}
              </option>
            ))}
          </select>

          <select
            className="select"
            style={{ width: "auto", fontSize: "0.8rem", padding: "0.3rem 0.5rem" }}
            value={conv.status}
            onChange={(e) => onStatus(e.target.value)}
            title="Status"
          >
            {Object.entries(STATUS_ID).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </select>

          <select
            className="select"
            style={{ width: "auto", fontSize: "0.8rem", padding: "0.3rem 0.5rem" }}
            value={conv.assigned_to ?? ""}
            onChange={(e) => onAssign(e.target.value || null)}
            title="Penerima tugas"
          >
            <option value="">Belum ditugaskan</option>
            {members.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </select>

          <button
            className="btn btn-sm"
            onClick={() => void inviteKichat()}
            title="Buat ruang chat privat di browser"
          >
            Lanjutkan di Kichat
          </button>
          <button className="icon-btn" onClick={onTogglePanel} title="Panel pelanggan">
            ☰
          </button>
        </div>
      </div>

      {kichatUrl && (
        <div className="alert alert-ok" style={{ margin: "0.6rem 1rem 0" }}>
          <strong>Tautan Kichat dibuat.</strong> Kirim ke pelanggan — berlaku 14 hari.
          <div className="secret-box">{kichatUrl}</div>
          <button
            className="btn btn-sm"
            onClick={() => void navigator.clipboard.writeText(kichatUrl)}
          >
            Copy
          </button>{" "}
          <button className="btn btn-sm" onClick={() => setKichatUrl(null)}>
            Tutup
          </button>
        </div>
      )}

      <div className="msgs" ref={bodyRef}>
        {items.map((it, i) =>
          "day" in it ? (
            <div className="day-sep" key={`d${i}`}>
              {it.day}
            </div>
          ) : (
            <div
              key={it.msg.id}
              className={
                "bubble " +
                (it.msg.direction === "inbound"
                  ? "bubble-in"
                  : it.msg.source === "ai_agent"
                    ? "bubble-ai"
                    : "bubble-out")
              }
            >
              {it.msg.body ?? <em className="faint">[{it.msg.msg_type}]</em>}
              <div className="bubble-meta">
                {it.msg.source === "ai_agent" && "◇ via AI Agent · "}
                {jam(it.msg.created_at)}
                {it.msg.direction === "outbound" && (
                  <>
                    {" "}
                    <Ticks status={it.msg.status} />
                  </>
                )}
                {it.msg.error_detail && ` · ${it.msg.error_detail}`}
              </div>
            </div>
          ),
        )}
        {messages.length === 0 && (
          <p className="faint" style={{ margin: "auto" }}>
            Belum ada pesan.
          </p>
        )}
      </div>

      <div className="composer-wrap">
        {matches.length > 0 && (
          <div className="shortcut-menu">
            {matches.map((c, i) => (
              <div
                key={c.id}
                className={"shortcut-item" + (i === pick ? " on" : "")}
                onMouseDown={(e) => {
                  e.preventDefault();
                  setDraft(c.body);
                }}
              >
                <strong className="mono">/{c.shortcut}</strong>
                <div className="faint" style={{ fontSize: "0.8rem" }}>
                  {c.body.slice(0, 90)}
                </div>
              </div>
            ))}
          </div>
        )}

        {conv.service_window_open ? (
          <div className="row">
            <input
              className="input"
              placeholder="Tulis pesan atau /shortcut…"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={onKey}
            />
            <button className="btn btn-primary" onClick={() => void send()} disabled={sending}>
              {sending ? "…" : "Kirim"}
            </button>
          </div>
        ) : (
          <p className="hint" style={{ margin: 0 }}>
            Jendela layanan 24 jam sudah tertutup — hanya template yang sudah
            disetujui yang bisa dikirim ke kontak ini.
          </p>
        )}
      </div>
    </div>
  );
}

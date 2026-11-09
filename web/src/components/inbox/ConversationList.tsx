import { useEffect, useRef } from "react";
import type { Conversation, InboxCounts } from "../../api";
import { inisial, PRIORITAS, waktuSingkat } from "../../api";

export function ConversationList({
  rows,
  counts,
  activeId,
  cursor,
  tab,
  search,
  onTab,
  onSearch,
  onOpen,
}: {
  rows: Conversation[];
  counts: InboxCounts | null;
  activeId: string | null;
  cursor: number;
  tab: "open" | "urgent";
  search: string;
  onTab: (t: "open" | "urgent") => void;
  onSearch: (s: string) => void;
  onOpen: (c: Conversation) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // Keep the keyboard cursor in view as it moves.
  useEffect(() => {
    const el = scrollRef.current?.querySelector<HTMLElement>(`[data-i="${cursor}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  return (
    <div className="list-col">
      <div className="list-head">
        <div className="list-tabs">
          <button className={"list-tab" + (tab === "open" ? " on" : "")} onClick={() => onTab("open")}>
            Terbuka {counts?.open ?? 0}
          </button>
          <button
            className={"list-tab" + (tab === "urgent" ? " on" : "")}
            onClick={() => onTab("urgent")}
          >
            Mendesak {counts?.urgent ?? 0}
          </button>
        </div>
        <input
          className="input"
          style={{ padding: "0.45rem 0.6rem", fontSize: "0.85rem" }}
          placeholder="Cari percakapan, kontak, atau nomor"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
        />
      </div>

      <div className="list-scroll" ref={scrollRef}>
        {rows.length === 0 ? (
          <p className="faint" style={{ padding: "1.5rem", textAlign: "center", fontSize: "0.88rem" }}>
            Tidak ada percakapan yang cocok.
          </p>
        ) : (
          rows.map((c, i) => (
            <div
              key={c.id}
              data-i={i}
              className={
                "crow" + (c.id === activeId ? " on" : "") + (i === cursor ? " cursor" : "")
              }
              onClick={() => onOpen(c)}
            >
              <span className="avatar">{inisial(c.contact.name, c.contact.wa_id)}</span>
              <div className="crow-main">
                <div className="crow-top">
                  <span className="crow-name">{c.contact.name ?? c.contact.wa_id}</span>
                  <span className="crow-time">{waktuSingkat(c.last_message_at)}</span>
                </div>
                <div className="crow-prev">{c.last_message_preview ?? "—"}</div>
                <div className="crow-meta">
                  <span
                    className="dot"
                    style={{ background: PRIORITAS[c.priority]?.color ?? "#6b779b" }}
                    title={PRIORITAS[c.priority]?.label}
                  />
                  {c.assignee_name && <span className="pill">{c.assignee_name}</span>}
                  {c.awaiting_reply && <span className="pill">belum dibalas</span>}
                  {c.labels.slice(0, 2).map((l) => (
                    <span key={l.id} className="pill pill-label" style={{ background: l.color }}>
                      {l.name}
                    </span>
                  ))}
                  {c.unread_count > 0 && (
                    <span className="unread" style={{ marginLeft: "auto" }}>
                      {c.unread_count}
                    </span>
                  )}
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      <div className="list-foot">
        <span>
          Menampilkan {rows.length} dari {counts?.total ?? rows.length} percakapan
        </span>
        <span>
          <kbd>↑</kbd> <kbd>↓</kbd> navigasi · <kbd>Enter</kbd> buka
        </span>
      </div>
    </div>
  );
}

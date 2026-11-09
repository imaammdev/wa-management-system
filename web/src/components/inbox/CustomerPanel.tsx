import { useEffect, useState } from "react";
import {
  api,
  inisial,
  rupiah,
  STATUS_ID,
  waktu,
  waktuSingkat,
  type Activity,
  type Conversation,
  type ContactPanel as Panel,
  type Label,
  type Note,
} from "../../api";

type Tab = "detail" | "aktivitas" | "catatan" | "deals";

const META_EVENTS = [
  { name: "LeadSubmitted", desc: "Tandai contact sebagai lead baru." },
  { name: "QualifiedLead", desc: "Tandai contact sebagai lead berkualitas." },
  { name: "ConvertedLead", desc: "Lead berhasil menjadi pelanggan." },
];

export function CustomerPanel({
  conv,
  labels,
  onClose,
  onChanged,
}: {
  conv: Conversation;
  labels: Label[];
  onClose: () => void;
  onChanged: () => void;
}) {
  const [tab, setTab] = useState<Tab>("detail");
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acts, setActs] = useState<Activity[]>([]);
  const [notes, setNotes] = useState<Note[]>([]);
  const [note, setNote] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const contactId = panel?.id ?? null;

  const loadPanel = async () => {
    setErr("");
    try {
      // The conversation carries the contact only by phone; the thread
      // endpoint gives us the id.
      const c = await api.get<{ conversation: { contact_id: string } }>(
        `/api/inbox/conversations/${conv.id}`,
      );
      const p = await api.get<{ contact: Panel }>(
        `/api/crm/contacts/${c.conversation.contact_id}/panel`,
      );
      setPanel(p.contact);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat pelanggan.");
    }
  };

  useEffect(() => {
    setPanel(null);
    setTab("detail");
    void loadPanel();
  }, [conv.id]);

  useEffect(() => {
    if (tab === "aktivitas") {
      void api
        .get<{ data: Activity[] }>(`/api/inbox/conversations/${conv.id}/activities`)
        .then((r) => setActs(r.data))
        .catch(() => {});
    }
    if (tab === "catatan") {
      void api
        .get<{ data: Note[] }>(`/api/inbox/conversations/${conv.id}/notes`)
        .then((r) => setNotes(r.data))
        .catch(() => {});
    }
  }, [tab, conv.id]);

  const sendMetaEvent = async (name: string) => {
    if (!contactId) return;
    setBusy(name);
    setErr("");
    try {
      await api.post(`/api/crm/contacts/${contactId}/meta-events`, {
        event_name: name,
        conversation_id: conv.id,
        wa_account_id: conv.wa_account_id,
      });
      await loadPanel();
      onChanged();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal mengirim event.");
    } finally {
      setBusy(null);
    }
  };

  const toggleLabel = async (labelId: string, on: boolean) => {
    if (!contactId) return;
    await api.post(`/api/crm/contacts/${contactId}/labels`, { label_id: labelId, on });
    await loadPanel();
  };

  const addNote = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!note.trim()) return;
    await api.post(`/api/inbox/conversations/${conv.id}/notes`, { body: note });
    setNote("");
    const r = await api.get<{ data: Note[] }>(`/api/inbox/conversations/${conv.id}/notes`);
    setNotes(r.data);
  };

  const addDeal = async () => {
    if (!contactId) return;
    const title = prompt("Judul deal?");
    if (!title) return;
    const value = Number(prompt("Nilai (Rp)?", "0") ?? 0);
    await api.post(`/api/crm/contacts/${contactId}/deals`, {
      title,
      value_idr: Number.isFinite(value) ? value : 0,
    });
    await loadPanel();
  };

  const name = panel?.name ?? panel?.profile_name ?? conv.contact.name;

  return (
    <aside className="panel">
      <div className="between" style={{ padding: "0.7rem 1rem 0" }}>
        <strong>Pelanggan</strong>
        <button className="icon-btn" onClick={onClose} title="Tutup">
          ✕
        </button>
      </div>

      <div className="panel-head">
        <div className="panel-avatar">{inisial(name ?? null, conv.contact.wa_id)}</div>
        <strong>{name ?? conv.contact.wa_id}</strong>
        <div className="faint mono" style={{ fontSize: "0.8rem" }}>
          +{conv.contact.wa_id}
        </div>
      </div>

      <div className="panel-tabs">
        {(["detail", "aktivitas", "catatan", "deals"] as Tab[]).map((t) => (
          <button key={t} className={"panel-tab" + (tab === t ? " on" : "")} onClick={() => setTab(t)}>
            {t === "detail" ? "Detail" : t === "aktivitas" ? "Aktivitas" : t === "catatan" ? "Catatan" : "Deals"}
          </button>
        ))}
      </div>

      <div className="panel-body">
        {err && <div className="alert">{err}</div>}

        {tab === "detail" && (
          <>
            <div className="panel-sec">Status</div>
            <div className="kv">
              <span className="badge badge-ok">{STATUS_ID[conv.status] ?? conv.status}</span>
            </div>
            <div className="kv">
              <span className="muted">Ditugaskan ke</span>
              <strong>{conv.assignee_name ?? "—"}</strong>
            </div>

            <div className="panel-sec">Nama</div>
            <div className="kv">
              <strong>{name ?? "—"}</strong>
            </div>

            <div className="panel-sec">Email</div>
            <div className="kv">
              <span className="muted">{panel?.email ?? "—"}</span>
            </div>

            <div className="panel-sec">Label</div>
            <div className="row wrap" style={{ gap: "0.3rem" }}>
              {labels.map((l) => {
                const on = panel?.labels.some((x) => x.id === l.id) ?? false;
                return (
                  <button
                    key={l.id}
                    className="pill"
                    onClick={() => void toggleLabel(l.id, !on)}
                    style={{
                      cursor: "pointer",
                      background: on ? l.color : "transparent",
                      color: on ? "#fff" : "var(--text-faint)",
                      borderColor: on ? l.color : "var(--border)",
                    }}
                  >
                    {l.name}
                  </button>
                );
              })}
              {labels.length === 0 && <span className="faint">Belum ada label.</span>}
            </div>

            <div className="panel-sec">Kirim event ke Meta</div>
            <p className="hint" style={{ marginTop: 0 }}>
              Laporkan milestone lead melalui Meta CTWA atau Meta CRM &amp; Chat.
            </p>
            {META_EVENTS.map((ev) => {
              const sent = panel?.meta_events.find((e) => e.event_name === ev.name);
              return (
                <div
                  key={ev.name}
                  style={{
                    border: "1px solid var(--border)",
                    borderRadius: "var(--radius-sm)",
                    padding: "0.6rem",
                    marginBottom: "0.5rem",
                  }}
                >
                  <strong style={{ fontSize: "0.85rem" }}>{ev.name}</strong>
                  <div className="faint" style={{ fontSize: "0.76rem", marginBottom: "0.4rem" }}>
                    {ev.desc}
                  </div>
                  {sent ? (
                    <span
                      className={
                        "badge " +
                        (sent.status === "sent"
                          ? "badge-ok"
                          : sent.status === "failed"
                            ? "badge-err"
                            : "badge-warn")
                      }
                    >
                      {sent.status === "sent"
                        ? `terkirim ${waktuSingkat(sent.sent_at)}`
                        : sent.status === "failed"
                          ? "gagal"
                          : "antre"}
                    </span>
                  ) : (
                    <button
                      className="btn btn-sm"
                      style={{ width: "100%", justifyContent: "center" }}
                      disabled={busy === ev.name || !contactId}
                      onClick={() => void sendMetaEvent(ev.name)}
                    >
                      {busy === ev.name ? "…" : "➤ Kirim ke Meta"}
                    </button>
                  )}
                </div>
              );
            })}

            <div className="panel-sec">Percakapan</div>
            <div className="kv">
              <span className="muted">Pesan terakhir</span>
              <strong>{waktuSingkat(conv.last_message_at) || "—"} lalu</strong>
            </div>
            <div className="kv">
              <span className="muted">Masuk terakhir</span>
              <strong>{panel ? waktuSingkat(panel.last_seen_at) || "—" : "—"} lalu</strong>
            </div>
          </>
        )}

        {tab === "aktivitas" && (
          <div className="timeline">
            {acts.length === 0 && <p className="faint">Belum ada aktivitas.</p>}
            {acts.map((a) => (
              <div className="timeline-item" key={a.id}>
                {a.summary}
                <div className="faint" style={{ fontSize: "0.74rem" }}>
                  {a.actor_name ?? "Sistem"} · {waktu(a.created_at)}
                </div>
              </div>
            ))}
          </div>
        )}

        {tab === "catatan" && (
          <>
            <form onSubmit={addNote} style={{ marginBottom: "0.8rem" }}>
              <textarea
                className="textarea"
                rows={3}
                placeholder="Catatan internal (tidak dikirim ke pelanggan)"
                value={note}
                onChange={(e) => setNote(e.target.value)}
              />
              <button className="btn btn-sm btn-primary" style={{ marginTop: "0.4rem" }}>
                Simpan catatan
              </button>
            </form>
            {notes.length === 0 && <p className="faint">Belum ada catatan.</p>}
            {notes.map((n) => (
              <div key={n.id} className="card" style={{ padding: "0.6rem", marginBottom: "0.5rem" }}>
                <div style={{ fontSize: "0.86rem", whiteSpace: "pre-wrap" }}>{n.body}</div>
                <div className="faint" style={{ fontSize: "0.73rem", marginTop: "0.3rem" }}>
                  {n.author_name ?? "—"} · {waktu(n.created_at)}
                </div>
              </div>
            ))}
          </>
        )}

        {tab === "deals" && (
          <>
            <button
              className="btn btn-sm btn-primary"
              style={{ width: "100%", justifyContent: "center", marginBottom: "0.7rem" }}
              onClick={() => void addDeal()}
              disabled={!contactId}
            >
              + Deal baru
            </button>
            {(panel?.deals.length ?? 0) === 0 && <p className="faint">Belum ada deal.</p>}
            {panel?.deals.map((d) => (
              <div key={d.id} className="card" style={{ padding: "0.6rem", marginBottom: "0.5rem" }}>
                <div className="between">
                  <strong style={{ fontSize: "0.88rem" }}>{d.title}</strong>
                  <span
                    className={
                      "badge " +
                      (d.status === "won" ? "badge-ok" : d.status === "lost" ? "badge-err" : "badge-warn")
                    }
                  >
                    {d.status}
                  </span>
                </div>
                <div className="faint" style={{ fontSize: "0.8rem" }}>
                  {rupiah(d.value_idr)}
                  {d.stage_name ? ` · ${d.stage_name}` : ""}
                </div>
              </div>
            ))}
          </>
        )}
      </div>
    </aside>
  );
}

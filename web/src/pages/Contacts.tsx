import { useEffect, useState } from "react";
import { api, waktu, type Contact, type Label } from "../api";
import { Empty, Err, PageHead } from "../components/bits";

const OPT_IN: Record<string, { text: string; cls: string }> = {
  opted_in: { text: "Berlangganan", cls: "badge badge-ok" },
  opted_out: { text: "Berhenti", cls: "badge badge-err" },
  unknown: { text: "Belum diketahui", cls: "badge" },
};

export function Contacts() {
  const [rows, setRows] = useState<Contact[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [err, setErr] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState({ name: "", email: "" });

  useEffect(() => {
    const t = setTimeout(() => setDebounced(search), 250);
    return () => clearTimeout(t);
  }, [search]);

  const load = async () => {
    try {
      const q = debounced.trim() ? `?search=${encodeURIComponent(debounced.trim())}` : "";
      const [c, l] = await Promise.all([
        api.get<{ data: Contact[] }>(`/api/contacts${q}`),
        api.get<{ data: Label[] }>("/api/contacts/labels"),
      ]);
      setRows(c.data);
      setLabels(l.data);
      setErr("");
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat kontak.");
    }
  };

  useEffect(() => {
    void load();
  }, [debounced]);

  const save = async (id: string) => {
    await api.patch(`/api/contacts/${id}`, {
      name: draft.name || null,
      email: draft.email || null,
    });
    setEditing(null);
    await load();
  };

  const optOut = async (id: string) => {
    if (!confirm("Tandai kontak ini berhenti berlangganan? Broadcast akan melewatinya.")) return;
    await api.post(`/api/contacts/${id}/opt-out`);
    await load();
  };

  return (
    <>
      <PageHead
        title="Contacts"
        sub="Daftar kontak WhatsApp beserta status berlangganan dan atributnya."
        action={
          <input
            className="input"
            style={{ width: "16rem" }}
            placeholder="Cari nama atau nomor…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        }
      />
      <Err>{err}</Err>

      <div className="row wrap" style={{ marginBottom: "1rem", gap: "0.4rem" }}>
        <span className="faint" style={{ fontSize: "0.82rem" }}>
          {rows.length} kontak
        </span>
        {labels.map((l) => (
          <span key={l.id} className="pill pill-label" style={{ background: l.color }}>
            {l.name}
          </span>
        ))}
      </div>

      {rows.length === 0 ? (
        <Empty>Belum ada kontak. Kontak dibuat otomatis saat pesan pertama masuk.</Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <table>
            <thead>
              <tr>
                <th>Nama</th>
                <th>Nomor</th>
                <th>Email</th>
                <th>Status</th>
                <th>Terakhir aktif</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <tr key={c.id}>
                  <td>
                    {editing === c.id ? (
                      <input
                        className="input"
                        value={draft.name}
                        onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                      />
                    ) : (
                      (c.name ?? c.profile_name ?? "—")
                    )}
                  </td>
                  <td className="mono">{c.wa_id}</td>
                  <td>
                    {editing === c.id ? (
                      <input
                        className="input"
                        value={draft.email}
                        onChange={(e) => setDraft({ ...draft, email: e.target.value })}
                      />
                    ) : (
                      <span className="faint">{c.email ?? "—"}</span>
                    )}
                  </td>
                  <td>
                    <span className={OPT_IN[c.opt_in_status]?.cls ?? "badge"}>
                      {OPT_IN[c.opt_in_status]?.text ?? c.opt_in_status}
                    </span>
                  </td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>
                    {waktu(c.last_seen_at)}
                  </td>
                  <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                    {editing === c.id ? (
                      <>
                        <button className="btn btn-sm btn-primary" onClick={() => void save(c.id)}>
                          Simpan
                        </button>{" "}
                        <button className="btn btn-sm" onClick={() => setEditing(null)}>
                          Batal
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          className="btn btn-sm"
                          onClick={() => {
                            setEditing(c.id);
                            setDraft({ name: c.name ?? "", email: c.email ?? "" });
                          }}
                        >
                          Edit
                        </button>{" "}
                        {c.opt_in_status !== "opted_out" && (
                          <button className="btn btn-sm btn-danger" onClick={() => void optOut(c.id)}>
                            Opt-out
                          </button>
                        )}
                      </>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

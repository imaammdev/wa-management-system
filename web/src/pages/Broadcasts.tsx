import { useEffect, useState } from "react";
import { api, waktu, type WaAccount } from "../api";
import { Empty, Err, Ok, PageHead, StatusBadge } from "../components/bits";

type Template = {
  id: string;
  name: string;
  language: string;
  category: string;
  status: string;
  wa_account_id: string;
};

type Broadcast = {
  id: string;
  name: string;
  status: string;
  total_recipients: number;
  sent_count: number;
  failed_count: number;
  scheduled_at: string | null;
  created_at: string;
};

export function Broadcasts() {
  const [rows, setRows] = useState<Broadcast[]>([]);
  const [numbers, setNumbers] = useState<WaAccount[]>([]);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [form, setForm] = useState({ name: "", wa_account_id: "", template_id: "" });
  const [err, setErr] = useState("");
  const [ok, setOk] = useState("");

  const load = async () => {
    try {
      const [b, n, t] = await Promise.all([
        api.get<{ data: Broadcast[] }>("/api/broadcasts"),
        api.get<{ data: WaAccount[] }>("/api/whatsapp-accounts"),
        api.get<{ data: Template[] }>("/api/templates"),
      ]);
      setRows(b.data);
      setNumbers(n.data);
      setTemplates(t.data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat broadcast.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const syncTemplates = async (waId: string) => {
    setErr("");
    setOk("");
    try {
      const r = await api.post<{ synced: number }>(`/api/templates/sync/${waId}`);
      setOk(`${r.synced} template disinkronkan dari provider.`);
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal sinkronisasi template.");
    }
  };

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setErr("");
    setOk("");
    try {
      const r = await api.post<{ total_recipients: number }>("/api/broadcasts", {
        ...form,
        variables: { "1": "{{contact.name}}" },
      });
      setOk(`Broadcast dibuat dengan ${r.total_recipients} penerima. Klik Jalankan untuk mulai.`);
      setForm({ name: "", wa_account_id: "", template_id: "" });
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal membuat broadcast.");
    }
  };

  const act = async (id: string, what: "start" | "pause") => {
    await api.post(`/api/broadcasts/${id}/${what}`);
    await load();
  };

  const approved = templates.filter((t) => t.status === "APPROVED");

  return (
    <>
      <PageHead
        title="Broadcast"
        sub="Kirim template yang sudah disetujui ke banyak kontak, dengan throttle dan opt-out otomatis."
      />
      <Err>{err}</Err>
      <Ok>{ok}</Ok>

      <div className="card" style={{ marginBottom: "1rem" }}>
        <div className="between wrap">
          <div>
            <strong style={{ fontSize: "0.95rem" }}>Template</strong>
            <p className="hint" style={{ margin: 0 }}>
              Template dibuat di Meta Business Manager, lalu disinkronkan ke sini.
              {templates.length > 0 && ` ${approved.length} dari ${templates.length} disetujui.`}
            </p>
          </div>
          <div className="row wrap">
            {numbers.map((n) => (
              <button key={n.id} className="btn btn-sm" onClick={() => void syncTemplates(n.id)}>
                Sync · {n.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <form className="card" onSubmit={create} style={{ marginBottom: "1.5rem" }}>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div style={{ flex: "1 1 14rem" }}>
            <label className="label">Nama kampanye</label>
            <input
              className="input"
              required
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="Promo November"
            />
          </div>
          <div style={{ flex: "1 1 12rem" }}>
            <label className="label">Nomor pengirim</label>
            <select
              className="select"
              required
              value={form.wa_account_id}
              onChange={(e) => setForm({ ...form, wa_account_id: e.target.value })}
            >
              <option value="">— pilih —</option>
              {numbers
                .filter((n) => n.status === "connected")
                .map((n) => (
                  <option key={n.id} value={n.id}>
                    {n.label}
                  </option>
                ))}
            </select>
          </div>
          <div style={{ flex: "1 1 12rem" }}>
            <label className="label">Template</label>
            <select
              className="select"
              required
              value={form.template_id}
              onChange={(e) => setForm({ ...form, template_id: e.target.value })}
            >
              <option value="">— pilih —</option>
              {approved.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name} ({t.language}) · {t.category}
                </option>
              ))}
            </select>
          </div>
          <button className="btn btn-primary" disabled={approved.length === 0}>
            Buat broadcast
          </button>
        </div>
        {approved.length === 0 && (
          <p className="hint">
            Belum ada template disetujui. Sinkronkan dulu dari nomor di atas.
          </p>
        )}
      </form>

      {rows.length === 0 ? (
        <Empty>Belum ada broadcast.</Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <table>
            <thead>
              <tr>
                <th>Nama</th>
                <th>Status</th>
                <th>Progress</th>
                <th>Dibuat</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((b) => (
                <tr key={b.id}>
                  <td>{b.name}</td>
                  <td>
                    <StatusBadge status={b.status} />
                  </td>
                  <td className="mono" style={{ fontSize: "0.82rem" }}>
                    {b.sent_count}/{b.total_recipients}
                    {b.failed_count > 0 && (
                      <span style={{ color: "var(--danger)" }}> · {b.failed_count} gagal</span>
                    )}
                  </td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>{waktu(b.created_at)}</td>
                  <td style={{ textAlign: "right" }}>
                    {["draft", "scheduled", "paused"].includes(b.status) && (
                      <button className="btn btn-sm btn-primary" onClick={() => void act(b.id, "start")}>
                        Jalankan
                      </button>
                    )}
                    {b.status === "running" && (
                      <button className="btn btn-sm" onClick={() => void act(b.id, "pause")}>
                        Jeda
                      </button>
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

import { useEffect, useState } from "react";
import { api, type CannedResponse, type Member } from "../api";
import { Err, Ok, PageHead } from "../components/bits";

type OrgSettings = {
  id: string;
  name: string;
  slug: string;
  timezone: string;
  locale: string;
  settings: {
    auto_assign?: "round_robin" | "off";
    auto_close_after_hours?: number;
    away_message?: string;
    business_hours?: { start?: string; end?: string; days?: number[] };
  };
};

type LabelRow = { id: string; name: string; color: string; usage: number };
type Stage = { id: string; name: string; position: number; deals: number };
type Pipeline = { id: string; name: string; is_default: boolean; stages: Stage[] };

type Payload = {
  organization: OrgSettings;
  labels: LabelRow[];
  canned_responses: CannedResponse[];
  pipelines: Pipeline[];
  members: Member[];
};

const TABS = [
  { key: "org", label: "Organisasi" },
  { key: "labels", label: "Label" },
  { key: "canned", label: "Balasan cepat" },
  { key: "pipelines", label: "Pipeline" },
  { key: "team", label: "Tim" },
] as const;
type Tab = (typeof TABS)[number]["key"];

const HARI = ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"];

export function Settings() {
  const [tab, setTab] = useState<Tab>("org");
  const [data, setData] = useState<Payload | null>(null);
  const [err, setErr] = useState("");
  const [ok, setOk] = useState("");

  const load = async () => {
    try {
      setData(await api.get<Payload>("/api/settings"));
      setErr("");
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat pengaturan.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const save = async (patch: Record<string, unknown>) => {
    setOk("");
    setErr("");
    try {
      await api.patch("/api/settings", patch);
      setOk("Tersimpan.");
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal menyimpan.");
    }
  };

  return (
    <>
      <PageHead title="Konfigurasi" sub="Pengaturan organisasi, label, balasan cepat, pipeline, dan tim." />
      <Err>{err}</Err>
      <Ok>{ok}</Ok>

      <div className="toggle" style={{ marginBottom: "1.25rem" }}>
        {TABS.map((t) => (
          <button key={t.key} className={tab === t.key ? "on" : ""} onClick={() => setTab(t.key)}>
            {t.label}
          </button>
        ))}
      </div>

      {!data ? (
        <p className="faint">Memuat…</p>
      ) : tab === "org" ? (
        <OrgTab org={data.organization} onSave={save} />
      ) : tab === "labels" ? (
        <LabelsTab labels={data.labels} reload={load} setErr={setErr} />
      ) : tab === "canned" ? (
        <CannedTab items={data.canned_responses} reload={load} setErr={setErr} />
      ) : tab === "pipelines" ? (
        <PipelinesTab pipelines={data.pipelines} reload={load} setErr={setErr} />
      ) : (
        <TeamTab members={data.members} />
      )}
    </>
  );
}

function OrgTab({
  org,
  onSave,
}: {
  org: OrgSettings;
  onSave: (patch: Record<string, unknown>) => Promise<void>;
}) {
  const [name, setName] = useState(org.name);
  const [timezone, setTimezone] = useState(org.timezone);
  const s = org.settings ?? {};
  const [autoAssign, setAutoAssign] = useState(s.auto_assign ?? "off");
  const [autoClose, setAutoClose] = useState(s.auto_close_after_hours ?? 0);
  const [away, setAway] = useState(s.away_message ?? "");
  const [start, setStart] = useState(s.business_hours?.start ?? "09:00");
  const [end, setEnd] = useState(s.business_hours?.end ?? "17:00");
  const [days, setDays] = useState<number[]>(s.business_hours?.days ?? [1, 2, 3, 4, 5]);

  return (
    <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(300px,1fr))" }}>
      <div className="card">
        <h3 style={{ fontSize: "1rem" }}>Profil</h3>
        <div className="field">
          <label className="label">Nama organisasi</label>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="field">
          <label className="label">Slug</label>
          <input className="input mono" value={org.slug} disabled />
          <div className="hint">Slug tidak bisa diubah setelah organisasi dibuat.</div>
        </div>
        <div className="field">
          <label className="label">Zona waktu</label>
          <select className="select" value={timezone} onChange={(e) => setTimezone(e.target.value)}>
            {["Asia/Jakarta", "Asia/Makassar", "Asia/Jayapura", "UTC"].map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        </div>
        <button className="btn btn-primary" onClick={() => void onSave({ name, timezone })}>
          Simpan profil
        </button>
      </div>

      <div className="card">
        <h3 style={{ fontSize: "1rem" }}>Jam operasional &amp; otomasi</h3>
        <div className="field">
          <label className="label">Jam kerja</label>
          <div className="row">
            <input
              className="input"
              type="time"
              value={start}
              onChange={(e) => setStart(e.target.value)}
            />
            <span className="faint">sampai</span>
            <input className="input" type="time" value={end} onChange={(e) => setEnd(e.target.value)} />
          </div>
        </div>
        <div className="field">
          <label className="label">Hari kerja</label>
          <div className="row wrap" style={{ gap: "0.3rem" }}>
            {HARI.map((h, i) => (
              <button
                key={h}
                className={"btn btn-sm" + (days.includes(i) ? " btn-primary" : "")}
                onClick={() =>
                  setDays((d) => (d.includes(i) ? d.filter((x) => x !== i) : [...d, i].sort()))
                }
              >
                {h}
              </button>
            ))}
          </div>
        </div>
        <div className="field">
          <label className="label">Pesan di luar jam kerja</label>
          <textarea
            className="textarea"
            rows={2}
            value={away}
            onChange={(e) => setAway(e.target.value)}
            placeholder="Terima kasih sudah menghubungi kami. Tim kami akan membalas pada jam kerja."
          />
        </div>
        <div className="field">
          <label className="label">Penugasan otomatis</label>
          <select
            className="select"
            value={autoAssign}
            onChange={(e) => setAutoAssign(e.target.value as "round_robin" | "off")}
          >
            <option value="off">Nonaktif</option>
            <option value="round_robin">Bergiliran (round robin)</option>
          </select>
        </div>
        <div className="field">
          <label className="label">Tutup otomatis setelah (jam)</label>
          <input
            className="input"
            type="number"
            min={0}
            value={autoClose}
            onChange={(e) => setAutoClose(Number(e.target.value))}
          />
          <div className="hint">0 menonaktifkan penutupan otomatis.</div>
        </div>
        <button
          className="btn btn-primary"
          onClick={() =>
            void onSave({
              settings: {
                auto_assign: autoAssign,
                auto_close_after_hours: autoClose,
                away_message: away,
                business_hours: { start, end, days },
              },
            })
          }
        >
          Simpan otomasi
        </button>
        <p className="hint">
          Jam kerja dan penugasan otomatis tersimpan, tapi belum dieksekusi oleh worker.
        </p>
      </div>
    </div>
  );
}

function LabelsTab({
  labels,
  reload,
  setErr,
}: {
  labels: LabelRow[];
  reload: () => Promise<void>;
  setErr: (s: string) => void;
}) {
  const [name, setName] = useState("");
  const [color, setColor] = useState("#2563eb");

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.post("/api/contacts/labels", { name, color });
      setName("");
      await reload();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal menambah label.");
    }
  };

  const remove = async (id: string, usage: number) => {
    if (!confirm(usage > 0 ? `Label ini dipakai ${usage} kali. Tetap hapus?` : "Hapus label ini?"))
      return;
    await api.del(`/api/settings/labels/${id}`);
    await reload();
  };

  return (
    <>
      <form className="card" onSubmit={add} style={{ marginBottom: "1rem" }}>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div style={{ flex: "1 1 14rem" }}>
            <label className="label">Nama label</label>
            <input className="input" required value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div>
            <label className="label">Warna</label>
            <input
              className="input"
              type="color"
              style={{ width: "4rem", padding: "0.2rem" }}
              value={color}
              onChange={(e) => setColor(e.target.value)}
            />
          </div>
          <button className="btn btn-primary">Tambah label</button>
        </div>
      </form>

      <div className="card" style={{ padding: 0, overflow: "hidden" }}>
        <table>
          <thead>
            <tr>
              <th>Label</th>
              <th style={{ textAlign: "right" }}>Dipakai</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {labels.map((l) => (
              <tr key={l.id}>
                <td>
                  <span className="row" style={{ gap: "0.5rem" }}>
                    <span className="dot" style={{ background: l.color, width: 11, height: 11 }} />
                    {l.name}
                  </span>
                </td>
                <td className="mono" style={{ textAlign: "right" }}>
                  {l.usage}
                </td>
                <td style={{ textAlign: "right" }}>
                  <button className="btn btn-sm btn-danger" onClick={() => void remove(l.id, l.usage)}>
                    Hapus
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

function CannedTab({
  items,
  reload,
  setErr,
}: {
  items: CannedResponse[];
  reload: () => Promise<void>;
  setErr: (s: string) => void;
}) {
  const [shortcut, setShortcut] = useState("");
  const [body, setBody] = useState("");

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.post("/api/inbox/canned-responses", { shortcut, body });
      setShortcut("");
      setBody("");
      await reload();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal menyimpan balasan.");
    }
  };

  return (
    <>
      <form className="card" onSubmit={add} style={{ marginBottom: "1rem" }}>
        <div className="field">
          <label className="label">Shortcut</label>
          <input
            className="input mono"
            required
            value={shortcut}
            onChange={(e) => setShortcut(e.target.value)}
            placeholder="resi"
          />
          <div className="hint">
            Agen mengetik <code>/{shortcut || "shortcut"}</code> di kolom pesan untuk menyisipkannya.
          </div>
        </div>
        <div className="field">
          <label className="label">Isi balasan</label>
          <textarea
            className="textarea"
            rows={3}
            required
            value={body}
            onChange={(e) => setBody(e.target.value)}
          />
        </div>
        <button className="btn btn-primary">Simpan balasan</button>
      </form>

      {items.length === 0 ? (
        <p className="faint">Belum ada balasan cepat.</p>
      ) : (
        <div className="stack">
          {items.map((c) => (
            <div className="card" key={c.id}>
              <div className="between">
                <strong className="mono">/{c.shortcut}</strong>
                <button
                  className="btn btn-sm btn-danger"
                  onClick={async () => {
                    await api.del(`/api/settings/canned-responses/${c.id}`);
                    await reload();
                  }}
                >
                  Hapus
                </button>
              </div>
              <p className="muted" style={{ fontSize: "0.88rem", margin: "0.4rem 0 0" }}>
                {c.body}
              </p>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function PipelinesTab({
  pipelines,
  reload,
  setErr,
}: {
  pipelines: Pipeline[];
  reload: () => Promise<void>;
  setErr: (s: string) => void;
}) {
  const addStage = async (id: string) => {
    const name = prompt("Nama tahap baru?");
    if (!name) return;
    try {
      await api.post(`/api/settings/pipelines/${id}/stages`, { name });
      await reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal menambah tahap.");
    }
  };

  return (
    <div className="stack">
      {pipelines.map((p) => (
        <div className="card" key={p.id}>
          <div className="between">
            <div className="row">
              <strong>{p.name}</strong>
              {p.is_default && <span className="badge badge-ok">default</span>}
            </div>
            <button className="btn btn-sm" onClick={() => void addStage(p.id)}>
              + Tahap
            </button>
          </div>
          <div className="row wrap" style={{ marginTop: "0.75rem", gap: "0.5rem" }}>
            {p.stages.map((s) => (
              <div
                key={s.id}
                className="card"
                style={{ padding: "0.5rem 0.75rem", minWidth: "8rem" }}
              >
                <div style={{ fontSize: "0.88rem", fontWeight: 600 }}>{s.name}</div>
                <div className="faint" style={{ fontSize: "0.76rem" }}>
                  {s.deals} deal
                </div>
              </div>
            ))}
            {p.stages.length === 0 && <span className="faint">Belum ada tahap.</span>}
          </div>
        </div>
      ))}
      {pipelines.length === 0 && <p className="faint">Belum ada pipeline.</p>}
    </div>
  );
}

function TeamTab({ members }: { members: Member[] }) {
  return (
    <div className="card" style={{ padding: 0, overflow: "hidden" }}>
      <table>
        <thead>
          <tr>
            <th>Nama</th>
            <th>Email</th>
            <th>Peran</th>
          </tr>
        </thead>
        <tbody>
          {members.map((m) => (
            <tr key={m.id}>
              <td>{m.name}</td>
              <td className="faint">{m.email}</td>
              <td>
                <span className="badge">{m.role}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

import { useEffect, useState } from "react";
import { api } from "../api";
import { Legend, LineChart, SegmentBar, Stat, type Point } from "../components/Chart";
import { Err, PageHead } from "../components/bits";

type Overview = {
  range: { days: number; from: string; to: string };
  volume: Array<{ day: string; inbound: number; outbound: number }>;
  delivery: {
    counts: { sent: number; delivered: number; read: number; failed: number; queued: number };
    delivery_rate: number;
    read_rate: number;
  };
  categories: Array<{ category: string | null; total: number }>;
  conversations: {
    opened: number;
    resolved: number;
    still_open: number;
    avg_first_response_secs: number | null;
    avg_resolution_secs: number | null;
  };
  agents: Array<{
    user_id: string;
    name: string;
    replies: number;
    resolved: number;
    avg_first_response_secs: number | null;
  }>;
  labels: Array<{ name: string; color: string; total: number }>;
  broadcasts: { campaigns: number; recipients: number; sent: number; failed: number };
};

const RANGES = [
  { days: 7, label: "7 hari" },
  { days: 30, label: "30 hari" },
  { days: 90, label: "90 hari" },
];

const CATEGORY_ID: Record<string, string> = {
  marketing: "Marketing",
  utility: "Utility",
  authentication: "Authentication",
  service: "Service",
};

/** "2 mnt 15 dtk" — response times are read by humans, not machines. */
function durasi(secs: number | null): string {
  if (secs === null || !Number.isFinite(secs)) return "—";
  const s = Math.round(secs);
  if (s < 60) return `${s} dtk`;
  if (s < 3600) return `${Math.floor(s / 60)} mnt ${s % 60} dtk`;
  if (s < 86400) return `${Math.floor(s / 3600)} jam ${Math.floor((s % 3600) / 60)} mnt`;
  return `${Math.floor(s / 86400)} hari ${Math.floor((s % 86400) / 3600)} jam`;
}

const persen = (f: number) => `${(f * 100).toFixed(1)}%`;

export function Analytics() {
  const [days, setDays] = useState(30);
  const [data, setData] = useState<Overview | null>(null);
  const [table, setTable] = useState(false);
  const [err, setErr] = useState("");

  useEffect(() => {
    setData(null);
    void api
      .get<Overview>(`/api/analytics/overview?days=${days}`)
      .then(setData)
      .catch((e) => setErr(e instanceof Error ? e.message : "Gagal memuat analitik."));
  }, [days]);

  const points: Point[] = (data?.volume ?? []).map((v) => ({
    label: new Intl.DateTimeFormat("id-ID", { day: "numeric", month: "short" }).format(
      new Date(v.day),
    ),
    values: [v.inbound, v.outbound],
  }));
  const names = ["Pesan masuk", "Pesan keluar"];

  const d = data?.delivery.counts;

  return (
    <div className="viz-root">
      <PageHead
        title="Analytics"
        sub="Volume pesan, performa pengiriman, dan beban kerja tim."
        action={
          <div className="toggle">
            {RANGES.map((r) => (
              <button key={r.days} className={days === r.days ? "on" : ""} onClick={() => setDays(r.days)}>
                {r.label}
              </button>
            ))}
          </div>
        }
      />
      <Err>{err}</Err>

      {!data ? (
        <p className="faint">Memuat…</p>
      ) : (
        <div className="stack">
          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px,1fr))" }}>
            <Stat
              label="Pesan keluar"
              value={(
                (d?.sent ?? 0) + (d?.delivered ?? 0) + (d?.read ?? 0) + (d?.failed ?? 0)
              ).toLocaleString("id-ID")}
              sub={`${data.delivery.counts.failed.toLocaleString("id-ID")} gagal`}
            />
            <Stat
              label="Delivery rate"
              value={persen(data.delivery.delivery_rate)}
              sub="sampai ke perangkat"
            />
            <Stat label="Read rate" value={persen(data.delivery.read_rate)} sub="dibaca penerima" />
            <Stat
              label="Rata-rata respons"
              value={durasi(data.conversations.avg_first_response_secs)}
              sub="balasan pertama"
            />
            <Stat
              label="Percakapan selesai"
              value={data.conversations.resolved.toLocaleString("id-ID")}
              sub={`${data.conversations.still_open.toLocaleString("id-ID")} masih terbuka`}
            />
          </div>

          <div className="card">
            <div className="between" style={{ marginBottom: "0.75rem" }}>
              <div>
                <h3 style={{ fontSize: "1rem", margin: 0 }}>Volume pesan per hari</h3>
                <Legend names={names} />
              </div>
              <button className="btn btn-sm" onClick={() => setTable((t) => !t)}>
                {table ? "Lihat grafik" : "Lihat tabel"}
              </button>
            </div>

            {table ? (
              <div style={{ maxHeight: 280, overflowY: "auto" }}>
                <table>
                  <thead>
                    <tr>
                      <th>Tanggal</th>
                      <th style={{ textAlign: "right" }}>Masuk</th>
                      <th style={{ textAlign: "right" }}>Keluar</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.volume.map((v) => (
                      <tr key={v.day}>
                        <td>{v.day}</td>
                        <td className="mono" style={{ textAlign: "right" }}>{v.inbound}</td>
                        <td className="mono" style={{ textAlign: "right" }}>{v.outbound}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <LineChart points={points} names={names} />
            )}
          </div>

          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(300px,1fr))" }}>
            <div className="card">
              <h3 style={{ fontSize: "1rem" }}>Kategori pesan (tarif Meta)</h3>
              <SegmentBar
                segments={data.categories
                  .filter((c) => c.total > 0)
                  .map((c) => ({
                    name: c.category ? (CATEGORY_ID[c.category] ?? c.category) : "Belum terklasifikasi",
                    value: c.total,
                  }))}
              />
            </div>

            <div className="card">
              <h3 style={{ fontSize: "1rem" }}>Status pengiriman</h3>
              <SegmentBar
                segments={[
                  { name: "Dibaca", value: d?.read ?? 0 },
                  { name: "Terkirim", value: d?.delivered ?? 0 },
                  { name: "Dikirim", value: d?.sent ?? 0 },
                  { name: "Gagal", value: d?.failed ?? 0 },
                ]}
              />
            </div>
          </div>

          <div className="card">
            <h3 style={{ fontSize: "1rem" }}>Performa agen</h3>
            {data.agents.length === 0 ? (
              <p className="faint">Belum ada data agen.</p>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Agen</th>
                    <th style={{ textAlign: "right" }}>Balasan</th>
                    <th style={{ textAlign: "right" }}>Selesai</th>
                    <th style={{ textAlign: "right" }}>Respons pertama</th>
                  </tr>
                </thead>
                <tbody>
                  {data.agents.map((a) => (
                    <tr key={a.user_id}>
                      <td>{a.name}</td>
                      <td className="mono" style={{ textAlign: "right" }}>
                        {a.replies.toLocaleString("id-ID")}
                      </td>
                      <td className="mono" style={{ textAlign: "right" }}>
                        {a.resolved.toLocaleString("id-ID")}
                      </td>
                      <td className="mono faint" style={{ textAlign: "right" }}>
                        {durasi(a.avg_first_response_secs)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(280px,1fr))" }}>
            <div className="card">
              <h3 style={{ fontSize: "1rem" }}>Broadcast</h3>
              <div className="kv">
                <span className="muted">Kampanye</span>
                <strong>{data.broadcasts.campaigns}</strong>
              </div>
              <div className="kv">
                <span className="muted">Penerima</span>
                <strong>{data.broadcasts.recipients.toLocaleString("id-ID")}</strong>
              </div>
              <div className="kv">
                <span className="muted">Terkirim</span>
                <strong>{data.broadcasts.sent.toLocaleString("id-ID")}</strong>
              </div>
              <div className="kv">
                <span className="muted">Gagal</span>
                <strong style={{ color: data.broadcasts.failed ? "var(--danger)" : undefined }}>
                  {data.broadcasts.failed.toLocaleString("id-ID")}
                </strong>
              </div>
            </div>

            <div className="card">
              <h3 style={{ fontSize: "1rem" }}>Label teratas</h3>
              {data.labels.length === 0 ? (
                <p className="faint">Belum ada label.</p>
              ) : (
                <table>
                  <tbody>
                    {data.labels.map((l) => (
                      <tr key={l.name}>
                        <td>
                          <span className="row" style={{ gap: "0.4rem" }}>
                            <span
                              className="dot"
                              style={{ background: l.color, width: 10, height: 10 }}
                            />
                            {l.name}
                          </span>
                        </td>
                        <td className="mono" style={{ textAlign: "right" }}>
                          {l.total.toLocaleString("id-ID")}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

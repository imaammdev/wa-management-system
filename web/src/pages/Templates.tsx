import { useEffect, useState } from "react";
import { api, waktu, type WaAccount } from "../api";
import { Empty, Err, Ok, PageHead, StatusBadge } from "../components/bits";

type Template = {
  id: string;
  wa_account_id: string;
  name: string;
  language: string;
  category: string;
  status: string;
  rejected_reason: string | null;
  components: unknown;
  created_at: string;
};

/** Pulls the BODY text out of Meta's component array for a readable preview. */
function bodyText(components: unknown): string | null {
  if (!Array.isArray(components)) return null;
  const body = components.find(
    (c) => typeof c === "object" && c !== null && (c as { type?: string }).type === "BODY",
  ) as { text?: string } | undefined;
  return body?.text ?? null;
}

export function Templates() {
  const [rows, setRows] = useState<Template[]>([]);
  const [numbers, setNumbers] = useState<WaAccount[]>([]);
  const [err, setErr] = useState("");
  const [ok, setOk] = useState("");
  const [filter, setFilter] = useState("all");

  const load = async () => {
    try {
      const [t, n] = await Promise.all([
        api.get<{ data: Template[] }>("/api/templates"),
        api.get<{ data: WaAccount[] }>("/api/whatsapp-accounts"),
      ]);
      setRows(t.data);
      setNumbers(n.data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat template.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const sync = async (id: string) => {
    setErr("");
    setOk("");
    try {
      const r = await api.post<{ synced: number }>(`/api/templates/sync/${id}`);
      setOk(`${r.synced} template disinkronkan.`);
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal sinkronisasi.");
    }
  };

  const shown = filter === "all" ? rows : rows.filter((r) => r.status === filter);
  const counts = {
    all: rows.length,
    APPROVED: rows.filter((r) => r.status === "APPROVED").length,
    PENDING: rows.filter((r) => r.status === "PENDING").length,
    REJECTED: rows.filter((r) => r.status === "REJECTED").length,
  };

  return (
    <>
      <PageHead
        title="Templates"
        sub="Template dibuat dan disetujui di Meta Business Manager, lalu disinkronkan ke sini."
        action={
          <div className="row wrap">
            {numbers.map((n) => (
              <button key={n.id} className="btn btn-sm" onClick={() => void sync(n.id)}>
                Sync · {n.label}
              </button>
            ))}
          </div>
        }
      />
      <Err>{err}</Err>
      <Ok>{ok}</Ok>

      <div className="toggle" style={{ marginBottom: "1rem" }}>
        {(["all", "APPROVED", "PENDING", "REJECTED"] as const).map((f) => (
          <button key={f} className={filter === f ? "on" : ""} onClick={() => setFilter(f)}>
            {f === "all" ? "Semua" : f === "APPROVED" ? "Disetujui" : f === "PENDING" ? "Menunggu" : "Ditolak"}{" "}
            {counts[f]}
          </button>
        ))}
      </div>

      {shown.length === 0 ? (
        <Empty>
          Belum ada template. Buat di Meta Business Manager, lalu tekan Sync pada nomor terkait.
        </Empty>
      ) : (
        <div className="stack">
          {shown.map((t) => (
            <div className="card" key={t.id}>
              <div className="between wrap">
                <div style={{ minWidth: 0 }}>
                  <div className="row wrap">
                    <strong className="mono">{t.name}</strong>
                    <span className="badge">{t.language}</span>
                    <span className="badge">{t.category}</span>
                    <StatusBadge status={t.status} />
                  </div>
                  {bodyText(t.components) && (
                    <p
                      className="muted"
                      style={{ fontSize: "0.88rem", margin: "0.5rem 0 0", whiteSpace: "pre-wrap" }}
                    >
                      {bodyText(t.components)}
                    </p>
                  )}
                  {t.rejected_reason && (
                    <p style={{ color: "var(--danger)", fontSize: "0.82rem", margin: "0.4rem 0 0" }}>
                      Ditolak: {t.rejected_reason}
                    </p>
                  )}
                </div>
                <span className="faint" style={{ fontSize: "0.78rem" }}>
                  {waktu(t.created_at)}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

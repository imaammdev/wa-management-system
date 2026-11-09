import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api, type Usage, type WaAccount } from "../api";
import { Meter, PageHead, StatusBadge } from "../components/bits";

export function Overview() {
  const [usage, setUsage] = useState<Usage | null>(null);
  const [numbers, setNumbers] = useState<WaAccount[]>([]);
  const [err, setErr] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        const [u, n] = await Promise.all([
          api.get<{ usage: Usage }>("/api/organizations/current/usage"),
          api.get<{ data: WaAccount[] }>("/api/whatsapp-accounts"),
        ]);
        setUsage(u.usage);
        setNumbers(n.data);
      } catch (e) {
        setErr(e instanceof Error ? e.message : "Gagal memuat data.");
      }
    })();
  }, []);

  return (
    <>
      <PageHead title="Ringkasan" sub="Pemakaian bulan ini dan status nomor Anda." />
      {err && <div className="alert">{err}</div>}

      <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(280px,1fr))" }}>
        <div className="card stack">
          <h3 style={{ fontSize: "0.95rem" }}>
            Kuota paket{" "}
            {usage && <span className="badge badge-ok">{usage.plan_code}</span>}
          </h3>
          {usage ? (
            <>
              <Meter label="Pesan terkirim" used={usage.messages_sent} limit={usage.messages_limit} />
              <Meter label="Broadcast" used={usage.broadcasts_sent} limit={usage.broadcasts_limit} />
              <Meter label="Nomor WhatsApp" used={usage.wa_accounts_used} limit={usage.wa_accounts_limit} />
              <Meter label="Member" used={usage.members_used} limit={usage.members_limit} />
            </>
          ) : (
            <p className="faint">Memuat…</p>
          )}
        </div>

        <div className="card">
          <div className="between" style={{ marginBottom: "0.75rem" }}>
            <h3 style={{ fontSize: "0.95rem", margin: 0 }}>Nomor WhatsApp</h3>
            <Link to="/app/nomor" className="btn btn-sm">
              Kelola
            </Link>
          </div>
          {numbers.length === 0 ? (
            <p className="faint" style={{ fontSize: "0.9rem" }}>
              Belum ada nomor. <Link to="/app/nomor">Hubungkan nomor pertama</Link> untuk
              mulai mengirim pesan.
            </p>
          ) : (
            <table>
              <tbody>
                {numbers.map((n) => (
                  <tr key={n.id}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{n.label}</div>
                      <div className="faint mono" style={{ fontSize: "0.78rem" }}>
                        {n.display_phone_number ?? "belum terhubung"}
                      </div>
                    </td>
                    <td style={{ textAlign: "right" }}>
                      <StatusBadge status={n.status} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="card">
          <h3 style={{ fontSize: "0.95rem" }}>Mulai cepat</h3>
          <ol className="muted" style={{ fontSize: "0.9rem", paddingLeft: "1.2rem", margin: 0 }}>
            <li>
              <Link to="/app/nomor">Hubungkan nomor WhatsApp</Link> (atau pakai provider
              <code> mock</code> untuk uji coba)
            </li>
            <li>
              <Link to="/app/api-keys">Buat API key</Link>
            </li>
            <li>
              <Link to="/app/webhooks">Daftarkan endpoint webhook</Link>
            </li>
            <li>
              Kirim pesan pertama ke <code>POST /v1/messages</code>
            </li>
          </ol>
        </div>
      </div>
    </>
  );
}

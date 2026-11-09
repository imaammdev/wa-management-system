import { useEffect, useState } from "react";
import { api, waktu, type DeliveryRow, type WebhookEndpointRow } from "../api";
import { Empty, Err, Ok, PageHead, StatusBadge } from "../components/bits";

const EVENTS = ["message.received", "message.status"];

export function Webhooks() {
  const [eps, setEps] = useState<WebhookEndpointRow[]>([]);
  const [dels, setDels] = useState<DeliveryRow[]>([]);
  const [url, setUrl] = useState("");
  const [events, setEvents] = useState<string[]>(EVENTS);
  const [secret, setSecret] = useState("");
  const [err, setErr] = useState("");

  const load = async () => {
    try {
      const [e, d] = await Promise.all([
        api.get<{ data: WebhookEndpointRow[] }>("/api/webhooks/endpoints"),
        api.get<{ data: DeliveryRow[] }>("/api/webhooks/deliveries?limit=50"),
      ]);
      setEps(e.data);
      setDels(d.data);
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal memuat webhook.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setErr("");
    try {
      const r = await api.post<{ secret: string }>("/api/webhooks/endpoints", { url, events });
      setSecret(r.secret);
      setUrl("");
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal membuat endpoint.");
    }
  };

  const remove = async (id: string) => {
    if (!confirm("Hapus endpoint ini?")) return;
    await api.del(`/api/webhooks/endpoints/${id}`);
    await load();
  };

  const replay = async (id: string) => {
    await api.post(`/api/webhooks/deliveries/${id}/replay`);
    await load();
  };

  return (
    <>
      <PageHead
        title="Webhooks"
        sub="Pesan masuk dan status delivery dikirim ke endpoint Anda dengan signature HMAC dan retry otomatis."
      />
      <Err>{err}</Err>

      {secret && (
        <Ok>
          <strong>Signing secret — simpan sekarang, hanya ditampilkan sekali.</strong>
          <div className="secret-box">{secret}</div>
          <p className="hint" style={{ margin: 0 }}>
            Verifikasi header <code>X-Mms-Signature-256</code>:
            <code> sha256=HMAC_SHA256(secret, raw_body)</code>
          </p>
          <button className="btn btn-sm" onClick={() => setSecret("")} style={{ marginTop: "0.5rem" }}>
            Tutup
          </button>
        </Ok>
      )}

      <form className="card" onSubmit={create} style={{ marginBottom: "1rem" }}>
        <div className="field">
          <label className="label">URL endpoint</label>
          <input
            className="input mono"
            required
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://app.anda.com/webhooks/mms"
          />
          <div className="hint">Harus HTTPS, kecuali http://localhost untuk development.</div>
        </div>
        <div className="field">
          <label className="label">Event</label>
          <div className="row wrap">
            {EVENTS.map((ev) => (
              <label key={ev} className="row" style={{ fontSize: "0.88rem" }}>
                <input
                  type="checkbox"
                  checked={events.includes(ev)}
                  onChange={(e) =>
                    setEvents(
                      e.target.checked ? [...events, ev] : events.filter((x) => x !== ev),
                    )
                  }
                />
                <code>{ev}</code>
              </label>
            ))}
          </div>
        </div>
        <button className="btn btn-primary" disabled={events.length === 0}>
          Tambah endpoint
        </button>
      </form>

      {eps.length === 0 ? (
        <Empty>Belum ada endpoint webhook.</Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden", marginBottom: "1.5rem" }}>
          <table>
            <thead>
              <tr>
                <th>URL</th>
                <th>Event</th>
                <th>Status</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {eps.map((e) => (
                <tr key={e.id}>
                  <td className="mono" style={{ fontSize: "0.82rem" }}>{e.url}</td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>{e.events.join(", ")}</td>
                  <td>
                    <StatusBadge status={e.active ? "active" : "disabled"} />
                  </td>
                  <td style={{ textAlign: "right" }}>
                    <button className="btn btn-sm btn-danger" onClick={() => void remove(e.id)}>
                      Hapus
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3 style={{ fontSize: "1rem" }}>Log pengiriman</h3>
      {dels.length === 0 ? (
        <Empty>Belum ada event terkirim.</Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <table>
            <thead>
              <tr>
                <th>Event</th>
                <th>Status</th>
                <th>Attempt</th>
                <th>HTTP</th>
                <th>Waktu</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {dels.map((d) => (
                <tr key={d.id}>
                  <td className="mono" style={{ fontSize: "0.82rem" }}>{d.event_type}</td>
                  <td>
                    <StatusBadge status={d.status} />
                  </td>
                  <td className="faint">
                    {d.attempt}/{d.max_attempts}
                  </td>
                  <td className="faint">{d.response_status ?? "—"}</td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>{waktu(d.created_at)}</td>
                  <td style={{ textAlign: "right" }}>
                    <button className="btn btn-sm" onClick={() => void replay(d.id)}>
                      Replay
                    </button>
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

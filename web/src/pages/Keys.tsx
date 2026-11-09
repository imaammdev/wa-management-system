import { useEffect, useState } from "react";
import { api, waktu, type ApiKeyRow } from "../api";
import { Empty, Err, Ok, PageHead } from "../components/bits";

export function Keys() {
  const [rows, setRows] = useState<ApiKeyRow[]>([]);
  const [name, setName] = useState("");
  const [secret, setSecret] = useState("");
  const [err, setErr] = useState("");

  const load = async () => {
    try {
      const r = await api.get<{ data: ApiKeyRow[] }>("/api/api-keys");
      setRows(r.data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat API key.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setErr("");
    try {
      const r = await api.post<{ secret: string }>("/api/api-keys", { name });
      setSecret(r.secret);
      setName("");
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal membuat API key.");
    }
  };

  const revoke = async (id: string) => {
    if (!confirm("Cabut API key ini? Integrasi yang memakainya akan langsung gagal.")) return;
    await api.post(`/api/api-keys/${id}/revoke`);
    await load();
  };

  return (
    <>
      <PageHead title="API Keys" sub="Autentikasi untuk endpoint /v1. Simpan key hanya di sisi server." />
      <Err>{err}</Err>

      {secret && (
        <Ok>
          <strong>Simpan sekarang — key ini tidak akan ditampilkan lagi.</strong>
          <div className="secret-box">{secret}</div>
          <button className="btn btn-sm" onClick={() => void navigator.clipboard.writeText(secret)}>
            Copy
          </button>{" "}
          <button className="btn btn-sm" onClick={() => setSecret("")}>
            Tutup
          </button>
        </Ok>
      )}

      <form className="card" onSubmit={create} style={{ marginBottom: "1rem" }}>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div style={{ flex: "1 1 16rem" }}>
            <label className="label">Nama key</label>
            <input
              className="input"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Server produksi"
            />
          </div>
          <button className="btn btn-primary">Buat API key</button>
        </div>
      </form>

      {rows.length === 0 ? (
        <Empty>Belum ada API key.</Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <table>
            <thead>
              <tr>
                <th>Nama</th>
                <th>Prefix</th>
                <th>Scopes</th>
                <th>Terakhir dipakai</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((k) => (
                <tr key={k.id} style={{ opacity: k.revoked_at ? 0.45 : 1 }}>
                  <td>{k.name}</td>
                  <td className="mono">{k.prefix}…</td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>
                    {k.scopes.join(", ")}
                  </td>
                  <td className="faint">{waktu(k.last_used_at)}</td>
                  <td style={{ textAlign: "right" }}>
                    {k.revoked_at ? (
                      <span className="badge badge-err">dicabut</span>
                    ) : (
                      <button className="btn btn-sm btn-danger" onClick={() => void revoke(k.id)}>
                        Cabut
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="card" style={{ marginTop: "1rem" }}>
        <h3 style={{ fontSize: "0.95rem" }}>Contoh pemakaian</h3>
        <pre className="secret-box" style={{ whiteSpace: "pre-wrap" }}>
{`curl -X POST http://localhost:8080/v1/messages \\
  -H "Authorization: Bearer mms_live_..." \\
  -H "Content-Type: application/json" \\
  -H "Idempotency-Key: $(uuidgen)" \\
  -d '{
    "wa_account_id": "<id nomor>",
    "to": "628123456789",
    "type": "text",
    "body": "Halo dari MMS!"
  }'`}
        </pre>
      </div>
    </>
  );
}

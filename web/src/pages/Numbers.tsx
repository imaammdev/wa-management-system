import { useEffect, useState } from "react";
import { api, type WaAccount } from "../api";
import { Empty, Err, Ok, PageHead, StatusBadge } from "../components/bits";

export function Numbers() {
  const [rows, setRows] = useState<WaAccount[]>([]);
  const [err, setErr] = useState("");
  const [ok, setOk] = useState("");
  const [label, setLabel] = useState("");
  const [provider, setProvider] = useState("mock");
  const [connecting, setConnecting] = useState<string | null>(null);
  const [creds, setCreds] = useState({ waba_id: "", phone_number_id: "", access_token: "" });
  const [sim, setSim] = useState({ from: "6281234567890", text: "Halo, saya mau tanya" });

  const load = async () => {
    try {
      const r = await api.get<{ data: WaAccount[] }>("/api/whatsapp-accounts");
      setRows(r.data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat nomor.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    setErr("");
    setOk("");
    try {
      await api.post("/api/whatsapp-accounts", { label, provider });
      setLabel("");
      setOk("Nomor dibuat. Lanjutkan dengan menghubungkan kredensial.");
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal membuat nomor.");
    }
  };

  const connect = async (id: string) => {
    setErr("");
    setOk("");
    try {
      await api.post(`/api/whatsapp-accounts/${id}/connect`, creds);
      setConnecting(null);
      setCreds({ waba_id: "", phone_number_id: "", access_token: "" });
      setOk("Nomor terhubung — kredensial diverifikasi ke provider.");
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal menghubungkan.");
    }
  };

  const connectMock = async (id: string) => {
    // The mock provider ignores credentials, so one click is enough to get a
    // working number for testing.
    setErr("");
    try {
      await api.post(`/api/whatsapp-accounts/${id}/connect`, {
        waba_id: "MOCK_WABA",
        phone_number_id: `mock-${id.slice(0, 8)}`,
        access_token: "mock-token",
      });
      setOk("Nomor mock terhubung. Anda bisa mengirim dan mensimulasikan pesan masuk.");
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal menghubungkan.");
    }
  };

  const simulate = async (id: string) => {
    setErr("");
    try {
      await api.post(`/api/whatsapp-accounts/${id}/simulate-inbound`, sim);
      setOk("Pesan masuk disimulasikan — cek halaman Inbox.");
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal mensimulasikan.");
    }
  };

  const remove = async (id: string) => {
    if (!confirm("Hapus nomor ini beserta seluruh percakapannya?")) return;
    try {
      await api.del(`/api/whatsapp-accounts/${id}`);
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal menghapus.");
    }
  };

  return (
    <>
      <PageHead
        title="Nomor WhatsApp"
        sub="Hubungkan WhatsApp Business Account dari Meta, atau pakai provider mock untuk uji coba."
      />
      <Err>{err}</Err>
      <Ok>{ok}</Ok>

      <form className="card" onSubmit={add} style={{ marginBottom: "1rem" }}>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div style={{ flex: "1 1 14rem" }}>
            <label className="label">Label</label>
            <input
              className="input"
              required
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="CS Utama"
            />
          </div>
          <div style={{ flex: "0 1 12rem" }}>
            <label className="label">Provider</label>
            <select className="select" value={provider} onChange={(e) => setProvider(e.target.value)}>
              <option value="mock">mock (uji coba)</option>
              <option value="meta_cloud">meta_cloud (resmi)</option>
            </select>
          </div>
          <button className="btn btn-primary">Tambah nomor</button>
        </div>
      </form>

      {rows.length === 0 ? (
        <Empty>Belum ada nomor WhatsApp.</Empty>
      ) : (
        <div className="stack">
          {rows.map((n) => (
            <div className="card" key={n.id}>
              <div className="between wrap">
                <div>
                  <div className="row">
                    <strong>{n.label}</strong>
                    <StatusBadge status={n.status} />
                    <span className="badge">{n.provider}</span>
                    {n.quality_rating && <span className="badge">{n.quality_rating}</span>}
                  </div>
                  <div className="faint mono" style={{ fontSize: "0.8rem", marginTop: "0.25rem" }}>
                    {n.display_phone_number ?? "—"}
                    {n.verified_name ? ` · ${n.verified_name}` : ""}
                  </div>
                </div>
                <div className="row wrap">
                  {n.status !== "connected" &&
                    (n.provider === "mock" ? (
                      <button className="btn btn-sm btn-primary" onClick={() => connectMock(n.id)}>
                        Hubungkan (mock)
                      </button>
                    ) : (
                      <button
                        className="btn btn-sm btn-primary"
                        onClick={() => setConnecting(connecting === n.id ? null : n.id)}
                      >
                        Hubungkan
                      </button>
                    ))}
                  {n.status === "connected" && n.provider === "mock" && (
                    <button className="btn btn-sm" onClick={() => simulate(n.id)}>
                      Simulasi pesan masuk
                    </button>
                  )}
                  <button className="btn btn-sm btn-danger" onClick={() => remove(n.id)}>
                    Hapus
                  </button>
                </div>
              </div>

              {connecting === n.id && (
                <div style={{ marginTop: "1rem", borderTop: "1px solid var(--border)", paddingTop: "1rem" }}>
                  <p className="hint" style={{ marginTop: 0 }}>
                    Ambil nilai ini dari Meta Business Manager. Token disimpan
                    terenkripsi AES-256-GCM dan diverifikasi sebelum disimpan.
                  </p>
                  <div className="field">
                    <label className="label">WABA ID</label>
                    <input
                      className="input mono"
                      value={creds.waba_id}
                      onChange={(e) => setCreds({ ...creds, waba_id: e.target.value })}
                    />
                  </div>
                  <div className="field">
                    <label className="label">Phone Number ID</label>
                    <input
                      className="input mono"
                      value={creds.phone_number_id}
                      onChange={(e) => setCreds({ ...creds, phone_number_id: e.target.value })}
                    />
                  </div>
                  <div className="field">
                    <label className="label">System User Access Token</label>
                    <input
                      className="input mono"
                      type="password"
                      value={creds.access_token}
                      onChange={(e) => setCreds({ ...creds, access_token: e.target.value })}
                    />
                  </div>
                  <button className="btn btn-primary" onClick={() => connect(n.id)}>
                    Simpan &amp; verifikasi
                  </button>
                </div>
              )}

              {n.status === "connected" && n.provider === "mock" && (
                <div className="row wrap" style={{ marginTop: "0.75rem" }}>
                  <input
                    className="input mono"
                    style={{ flex: "0 1 12rem" }}
                    value={sim.from}
                    onChange={(e) => setSim({ ...sim, from: e.target.value })}
                  />
                  <input
                    className="input"
                    style={{ flex: "1 1 14rem" }}
                    value={sim.text}
                    onChange={(e) => setSim({ ...sim, text: e.target.value })}
                  />
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  );
}

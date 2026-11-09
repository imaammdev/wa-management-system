import { useState } from "react";
import { api, setCurrentOrgId } from "../api";
import { Err } from "../components/bits";

/** Shown when a signed-in user has no organization yet (e.g. invited then removed). */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr("");
    try {
      const r = await api.post<{ organization: { id: string } }>(
        "/api/organizations",
        { name },
      );
      setCurrentOrgId(r.organization.id);
      onDone();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal membuat organisasi.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="center-page">
      <form className="card auth-card" onSubmit={submit}>
        <h2>Buat organisasi</h2>
        <p className="hint" style={{ marginTop: 0, marginBottom: "1rem" }}>
          Organisasi adalah wadah untuk nomor WhatsApp, tim, dan klien Anda.
        </p>
        <Err>{err}</Err>
        <div className="field">
          <label className="label">Nama organisasi</label>
          <input className="input" required value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <button className="btn btn-primary" disabled={busy} style={{ width: "100%", justifyContent: "center" }}>
          {busy ? "Membuat…" : "Lanjutkan"}
        </button>
      </form>
    </div>
  );
}

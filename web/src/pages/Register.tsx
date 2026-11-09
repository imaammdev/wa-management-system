import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api";
import { Err } from "../components/bits";

export function Register({ onDone }: { onDone: () => void }) {
  const [form, setForm] = useState({
    name: "",
    email: "",
    password: "",
    org_name: "",
  });
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr("");
    try {
      await api.post("/api/auth/register", form);
      onDone();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal mendaftar.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="center-page">
      <form className="card auth-card" onSubmit={submit}>
        <h2>Daftar</h2>
        <p className="hint" style={{ marginTop: 0, marginBottom: "1rem" }}>
          Gratis 14 hari di paket Starter. Tanpa kartu kredit.
        </p>
        <Err>{err}</Err>
        <div className="field">
          <label className="label">Nama</label>
          <input className="input" required value={form.name} onChange={set("name")} />
        </div>
        <div className="field">
          <label className="label">Email</label>
          <input
            className="input"
            type="email"
            required
            autoComplete="email"
            value={form.email}
            onChange={set("email")}
          />
        </div>
        <div className="field">
          <label className="label">Password</label>
          <input
            className="input"
            type="password"
            required
            minLength={8}
            autoComplete="new-password"
            value={form.password}
            onChange={set("password")}
          />
          <div className="hint">Minimal 8 karakter.</div>
        </div>
        <div className="field">
          <label className="label">Nama organisasi</label>
          <input
            className="input"
            required
            value={form.org_name}
            onChange={set("org_name")}
            placeholder="PT Contoh Jaya"
          />
        </div>
        <button className="btn btn-primary" disabled={busy} style={{ width: "100%", justifyContent: "center" }}>
          {busy ? "Memproses…" : "Buat akun"}
        </button>
        <p className="hint" style={{ textAlign: "center", marginTop: "1rem" }}>
          Sudah punya akun? <Link to="/masuk">Masuk</Link>
        </p>
      </form>
    </div>
  );
}

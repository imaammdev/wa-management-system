import { useState } from "react";
import { Link } from "react-router-dom";
import { api, ApiError } from "../api";
import { Err } from "../components/bits";

export function Login({ onDone }: { onDone: () => void }) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr("");
    try {
      await api.post("/api/auth/login", { email, password });
      onDone();
    } catch (ex) {
      setErr(
        ex instanceof ApiError && ex.status === 401
          ? "Email atau password salah."
          : ex instanceof Error
            ? ex.message
            : "Gagal masuk.",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="center-page">
      <form className="card auth-card" onSubmit={submit}>
        <h2>Masuk</h2>
        <Err>{err}</Err>
        <div className="field">
          <label className="label">Email</label>
          <input
            className="input"
            type="email"
            required
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </div>
        <div className="field">
          <label className="label">Password</label>
          <input
            className="input"
            type="password"
            required
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
        <button className="btn btn-primary" disabled={busy} style={{ width: "100%", justifyContent: "center" }}>
          {busy ? "Memproses…" : "Masuk"}
        </button>
        <p className="hint" style={{ textAlign: "center", marginTop: "1rem" }}>
          Belum punya akun? <Link to="/daftar">Daftar</Link>
        </p>
      </form>
    </div>
  );
}

import { useEffect, useState } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { api, currentOrgId, setCurrentOrgId, type Org, type User } from "./api";
import { Layout } from "./components/Layout";
import { Landing } from "./pages/Landing";
import { Login } from "./pages/Login";
import { Register } from "./pages/Register";
import { Onboarding } from "./pages/Onboarding";
import { Inbox } from "./pages/Inbox";
import { Numbers } from "./pages/Numbers";
import { Keys } from "./pages/Keys";
import { Webhooks } from "./pages/Webhooks";
import { Broadcasts } from "./pages/Broadcasts";
import { Billing } from "./pages/Billing";
import { Overview } from "./pages/Overview";
import { Analytics } from "./pages/Analytics";
import { Contacts } from "./pages/Contacts";
import { Templates } from "./pages/Templates";
import { Settings } from "./pages/Settings";
import { AiAgent } from "./pages/AiAgent";
import { Ads } from "./pages/Ads";
import { Kichat } from "./pages/Kichat";

export type Session = { user: User; organizations: Org[] } | null;

export function App() {
  const [session, setSession] = useState<Session>(null);
  const [loading, setLoading] = useState(true);

  const refresh = async () => {
    try {
      const me = await api.get<{ user: User; organizations: Org[] }>(
        "/api/auth/me",
      );
      setSession(me);
      // Keep the active org pointing at something that still exists.
      const current = currentOrgId();
      if (!current || !me.organizations.some((o) => o.id === current)) {
        setCurrentOrgId(me.organizations[0]?.id ?? null);
      }
    } catch {
      setSession(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  if (loading) {
    return (
      <div className="center-page">
        <p className="muted">Memuat…</p>
      </div>
    );
  }

  const signedIn = session !== null;
  const hasOrg = (session?.organizations.length ?? 0) > 0;

  return (
    <Routes>
      <Route path="/" element={<Landing signedIn={signedIn} />} />
      {/* Customer-facing; deliberately outside the auth guard. */}
      <Route path="/kichat/:token" element={<Kichat />} />
      <Route
        path="/masuk"
        element={
          signedIn ? <Navigate to="/app" replace /> : <Login onDone={refresh} />
        }
      />
      <Route
        path="/daftar"
        element={
          signedIn ? (
            <Navigate to="/app" replace />
          ) : (
            <Register onDone={refresh} />
          )
        }
      />

      {!signedIn && <Route path="/app/*" element={<Navigate to="/masuk" replace />} />}

      {signedIn && !hasOrg && (
        <Route path="/app/*" element={<Onboarding onDone={refresh} />} />
      )}

      {signedIn && hasOrg && (
        <Route
          path="/app"
          element={<Layout session={session!} onSwitchOrg={refresh} />}
        >
          <Route index element={<Overview />} />
          <Route path="inbox" element={<Inbox />} />
          <Route path="nomor" element={<Numbers />} />
          <Route path="api-keys" element={<Keys />} />
          <Route path="webhooks" element={<Webhooks />} />
          <Route path="kontak" element={<Contacts />} />
          <Route path="template" element={<Templates />} />
          <Route path="broadcast" element={<Broadcasts />} />
          <Route path="analytics" element={<Analytics />} />
          <Route path="ai-agent" element={<AiAgent />} />
          <Route path="ads" element={<Ads />} />
          <Route path="konfigurasi" element={<Settings />} />
          <Route path="tagihan" element={<Billing />} />
        </Route>
      )}

      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

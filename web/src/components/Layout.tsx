import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { api, currentOrgId, setCurrentOrgId } from "../api";
import { useTheme } from "./theme";
import type { Session } from "../App";

const NAV = [
  { to: "/app", label: "Ringkasan", end: true },
  { to: "/app/inbox", label: "Inbox" },
  { to: "/app/kontak", label: "Contacts" },
  { to: "/app/template", label: "Templates" },
  { to: "/app/broadcast", label: "Broadcast" },
  { to: "/app/analytics", label: "Analytics" },
  { to: "/app/ai-agent", label: "AI Agent" },
  { to: "/app/ads", label: "Ads" },
  { to: "/app/nomor", label: "Nomor WhatsApp" },
  { to: "/app/api-keys", label: "API Keys" },
  { to: "/app/webhooks", label: "Webhooks" },
  { to: "/app/konfigurasi", label: "Konfigurasi" },
  { to: "/app/tagihan", label: "Paket & Tagihan" },
];

export function Layout({
  session,
  onSwitchOrg,
}: {
  session: NonNullable<Session>;
  onSwitchOrg: () => void;
}) {
  const navigate = useNavigate();
  const [theme, toggleTheme] = useTheme();
  const orgId = currentOrgId();
  const org = session.organizations.find((o) => o.id === orgId);

  const logout = async () => {
    await api.post("/api/auth/logout");
    setCurrentOrgId(null);
    window.location.href = "/";
  };

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="sidebar-nav">
          <div className="brand">
            M<span>MS</span>
          </div>
          {NAV.map((n) => (
            <NavLink
              key={n.to}
              to={n.to}
              end={n.end}
              className={({ isActive }) =>
                "navlink" + (isActive ? " active" : "")
              }
            >
              {n.label}
            </NavLink>
          ))}
        </div>

        <div className="sidebar-foot">
          {session.organizations.length > 1 && (
            <select
              className="select"
              value={orgId ?? ""}
              onChange={(e) => {
                setCurrentOrgId(e.target.value);
                onSwitchOrg();
                navigate("/app");
              }}
              style={{ fontSize: "0.85rem" }}
            >
              {session.organizations.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </select>
          )}
          <div className="faint sidebar-who">
            {session.user.email}
            {org && (
              <>
                <br />
                {org.name} · {org.role}
              </>
            )}
          </div>
          <div className="row" style={{ gap: "0.4rem" }}>
            <button
              className="btn btn-sm sidebar-logout"
              onClick={logout}
              style={{ flex: 1 }}
            >
              Keluar
            </button>
            <button
              className="btn btn-sm"
              onClick={toggleTheme}
              title={theme === "dark" ? "Mode terang" : "Mode gelap"}
              aria-label="Ganti tema"
            >
              {theme === "dark" ? "☀" : "☾"}
            </button>
          </div>
        </div>
      </aside>

      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}

import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { api, currentOrgId, setCurrentOrgId } from "../api";
import type { Session } from "../App";

const NAV = [
  { to: "/app", label: "Ringkasan", end: true },
  { to: "/app/inbox", label: "Inbox" },
  { to: "/app/nomor", label: "Nomor WhatsApp" },
  { to: "/app/broadcast", label: "Broadcast" },
  { to: "/app/api-keys", label: "API Keys" },
  { to: "/app/webhooks", label: "Webhooks" },
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
        <div>
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

        <div style={{ marginTop: "auto", paddingTop: "1rem" }}>
          {session.organizations.length > 1 && (
            <select
              className="select"
              value={orgId ?? ""}
              onChange={(e) => {
                setCurrentOrgId(e.target.value);
                onSwitchOrg();
                navigate("/app");
              }}
              style={{ marginBottom: "0.5rem", fontSize: "0.85rem" }}
            >
              {session.organizations.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </select>
          )}
          <div className="faint" style={{ fontSize: "0.78rem", padding: "0 0.65rem" }}>
            {session.user.email}
            {org && (
              <>
                <br />
                {org.name} · {org.role}
              </>
            )}
          </div>
          <button
            className="btn btn-sm"
            onClick={logout}
            style={{ marginTop: "0.5rem", width: "100%" }}
          >
            Keluar
          </button>
        </div>
      </aside>

      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}

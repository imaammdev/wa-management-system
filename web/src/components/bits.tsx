import type { ReactNode } from "react";

export function PageHead({
  title,
  sub,
  action,
}: {
  title: string;
  sub?: string;
  action?: ReactNode;
}) {
  return (
    <div className="page-head between">
      <div>
        <h2 style={{ marginBottom: 0 }}>{title}</h2>
        {sub && <p>{sub}</p>}
      </div>
      {action}
    </div>
  );
}

export function StatusBadge({ status }: { status: string }) {
  const ok = ["connected", "delivered", "read", "sent", "active", "APPROVED", "completed"];
  const warn = ["pending", "queued", "trialing", "running", "scheduled", "PENDING", "paused"];
  const cls = ok.includes(status)
    ? "badge badge-ok"
    : warn.includes(status)
      ? "badge badge-warn"
      : ["failed", "error", "exhausted", "REJECTED", "disconnected"].includes(status)
        ? "badge badge-err"
        : "badge";
  return <span className={cls}>{status}</span>;
}

export function Err({ children }: { children: ReactNode }) {
  if (!children) return null;
  return <div className="alert">{children}</div>;
}

export function Ok({ children }: { children: ReactNode }) {
  if (!children) return null;
  return <div className="alert alert-ok">{children}</div>;
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="card" style={{ textAlign: "center", color: "var(--text-faint)" }}>
      {children}
    </div>
  );
}

/** Usage meter with the limit made explicit — quota surprises are the #1 support ticket. */
export function Meter({
  label,
  used,
  limit,
}: {
  label: string;
  used: number;
  limit: number;
}) {
  const pct = limit > 0 ? Math.min(100, (used / limit) * 100) : 0;
  const color = pct > 90 ? "var(--danger)" : pct > 70 ? "var(--warn)" : "var(--accent)";
  return (
    <div>
      <div className="between" style={{ fontSize: "0.85rem", marginBottom: "0.3rem" }}>
        <span className="muted">{label}</span>
        <span className="mono">
          {used.toLocaleString("id-ID")} / {limit.toLocaleString("id-ID")}
        </span>
      </div>
      <div
        style={{
          height: 6,
          background: "var(--bg)",
          borderRadius: 999,
          overflow: "hidden",
        }}
      >
        <div style={{ width: `${pct}%`, height: "100%", background: color }} />
      </div>
    </div>
  );
}

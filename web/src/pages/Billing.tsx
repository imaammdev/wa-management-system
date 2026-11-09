import { useEffect, useState } from "react";
import { api, rupiah, waktu, type Plan } from "../api";
import { Err, Ok, PageHead, StatusBadge } from "../components/bits";

type Subscription = {
  status: string;
  interval: string;
  current_period_end: string;
  trial_ends_at: string | null;
  plan_id: string;
};

export function Billing() {
  const [sub, setSub] = useState<Subscription | null>(null);
  const [plans, setPlans] = useState<Plan[]>([]);
  const [interval, setInterval] = useState<"monthly" | "yearly">("monthly");
  const [err, setErr] = useState("");
  const [ok, setOk] = useState("");

  const load = async () => {
    try {
      const r = await api.get<{ subscription: Subscription | null; plans: Plan[] }>(
        "/api/billing/subscription",
      );
      setSub(r.subscription);
      setPlans(r.plans);
      if (r.subscription) setInterval(r.subscription.interval as "monthly" | "yearly");
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat langganan.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const change = async (code: string) => {
    setErr("");
    setOk("");
    try {
      await api.post("/api/billing/subscription/change-plan", {
        plan_code: code,
        interval,
      });
      setOk("Paket diperbarui.");
      await load();
    } catch (ex) {
      setErr(ex instanceof Error ? ex.message : "Gagal mengganti paket.");
    }
  };

  const currentPlan = plans.find((p) => p.id === sub?.plan_id);

  return (
    <>
      <PageHead title="Paket & Tagihan" sub="Ganti paket kapan saja. Biaya pesan ditagih langsung oleh Meta." />
      <Err>{err}</Err>
      <Ok>{ok}</Ok>

      {sub && (
        <div className="card" style={{ marginBottom: "1.5rem" }}>
          <div className="between wrap">
            <div>
              <div className="row">
                <strong>{currentPlan?.name ?? "—"}</strong>
                <StatusBadge status={sub.status} />
                <span className="badge">{sub.interval}</span>
              </div>
              <p className="hint" style={{ margin: "0.3rem 0 0" }}>
                {sub.status === "trialing" && sub.trial_ends_at
                  ? `Masa percobaan berakhir ${waktu(sub.trial_ends_at)}`
                  : `Periode berjalan sampai ${waktu(sub.current_period_end)}`}
              </p>
            </div>
            <div className="toggle">
              <button className={interval === "monthly" ? "on" : ""} onClick={() => setInterval("monthly")}>
                Bulanan
              </button>
              <button className={interval === "yearly" ? "on" : ""} onClick={() => setInterval("yearly")}>
                Tahunan
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="pricing">
        {plans.map((p) => {
          const active = p.id === sub?.plan_id && p.id !== undefined;
          return (
            <div className={"card plan" + (p.is_popular ? " popular" : "")} key={p.code}>
              {p.is_popular && <span className="plan-tag">Paling Populer</span>}
              <h3>{p.name}</h3>
              <p className="faint" style={{ fontSize: "0.85rem", margin: 0 }}>{p.tagline}</p>
              <div className="price" style={{ marginTop: "0.75rem" }}>
                {rupiah(
                  interval === "yearly"
                    ? Math.round(p.price_idr_yearly / 12)
                    : p.price_idr_monthly,
                )}
                <small>/bulan</small>
              </div>
              <ul>
                <li>{p.max_messages_per_month.toLocaleString("id-ID")} pesan/bulan</li>
                <li>{p.max_broadcasts_per_month.toLocaleString("id-ID")} broadcast/bulan</li>
                <li>{p.max_wa_accounts} nomor WhatsApp</li>
                <li>{p.max_members} member</li>
                <li>Riwayat {p.history_retention_days} hari</li>
              </ul>
              <button
                className={"btn" + (active ? "" : " btn-primary")}
                disabled={active && interval === sub?.interval}
                onClick={() => void change(p.code)}
                style={{ width: "100%", justifyContent: "center" }}
              >
                {active && interval === sub?.interval ? "Paket aktif" : `Pilih ${p.name}`}
              </button>
            </div>
          );
        })}
      </div>

      <p className="hint" style={{ marginTop: "1.5rem" }}>
        Pembayaran otomatis (Midtrans/Xendit) belum diaktifkan di environment ini —
        penggantian paket berlaku langsung.
      </p>
    </>
  );
}

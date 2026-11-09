import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { api, rupiah, type Plan } from "../api";

type MetaRate = {
  category: string;
  rate_idr: number;
  free_tier_per_number_per_month: number;
};

const FEATURES = [
  {
    t: "Kirim Pesan",
    d: "Text, image, video, document, template, interactive — semua tipe pesan WhatsApp lewat satu endpoint REST.",
  },
  {
    t: "Webhook Real-time",
    d: "Pesan masuk dan status update dikirim dengan retry otomatis 8 percobaan dan verifikasi signature HMAC.",
  },
  {
    t: "Shared Inbox",
    d: "Satu nomor untuk seluruh tim CS. Assign otomatis, label, dan pipeline tanpa saling lempar HP.",
  },
  {
    t: "Multi-tenant",
    d: "Organization → Teams → Members → nomor WhatsApp. RBAC dan isolasi data antar klien Anda.",
  },
  {
    t: "Aman Sejak Awal",
    d: "Token provider dienkripsi AES-256-GCM. Rate limiting, idempotency key, dan audit log bawaan.",
  },
  {
    t: "Broadcast Terkendali",
    d: "Kirim template ke ribuan kontak dengan throttle per menit, dan opt-out dihormati otomatis.",
  },
];

const FAQ = [
  {
    q: "Apakah semua paket bisa dipakai multi-tenant?",
    a: "Ya. Semua paket bersifat multi-tenant, jadi Anda bisa mengelola banyak klien dalam satu akun. Yang membedakan paket hanya skala kuotanya.",
  },
  {
    q: "Apakah biaya pesan sudah termasuk?",
    a: "Belum. Biaya langganan di atas adalah biaya platform. Pesan ditagih langsung oleh Meta ke akun WhatsApp Business Anda sesuai kategori pesan.",
  },
  {
    q: "Apa itu service message?",
    a: "Balasan biasa dalam 24 jam setelah pelanggan mengirim pesan. Setiap nomor mendapat kuota gratis per bulan; di luar jendela 24 jam Anda harus memakai template yang sudah disetujui.",
  },
  {
    q: "Bisa menambah nomor di luar kuota paket?",
    a: "Bisa. Semua paket dapat membeli add-on Extra WhatsApp Number sebanyak yang dibutuhkan.",
  },
];

export function Landing({ signedIn }: { signedIn: boolean }) {
  const [plans, setPlans] = useState<Plan[]>([]);
  const [rates, setRates] = useState<MetaRate[]>([]);
  const [yearly, setYearly] = useState(false);

  useEffect(() => {
    void api.get<{ data: Plan[] }>("/public/plans").then((r) => setPlans(r.data)).catch(() => {});
    void api
      .get<{ data: MetaRate[] }>("/public/meta-rates")
      .then((r) => setRates(r.data))
      .catch(() => {});
  }, []);

  return (
    <>
      <nav className="lp-nav">
        <strong style={{ letterSpacing: "-0.03em" }}>
          M<span style={{ color: "var(--accent)" }}>MS</span>
        </strong>
        <div className="row">
          <a href="#harga" className="muted">
            Harga
          </a>
          <a href="#faq" className="muted">
            FAQ
          </a>
          {signedIn ? (
            <Link to="/app" className="btn btn-sm btn-primary">
              Buka Dashboard
            </Link>
          ) : (
            <>
              <Link to="/masuk" className="btn btn-sm">
                Masuk
              </Link>
              <Link to="/daftar" className="btn btn-sm btn-primary">
                Mulai Sekarang
              </Link>
            </>
          )}
        </div>
      </nav>

      <div className="lp">
        <section className="hero">
          <span className="eyebrow">Harga mulai 25rb</span>
          <h1>
            WhatsApp API untuk
            <br />
            Developer &amp; Tim CS
          </h1>
          <p className="lede">
            Hubungkan WhatsApp Cloud API resmi Meta ke REST API, webhook, shared
            inbox, dan otomasi dalam satu platform.
          </p>
          <div className="terminal">
            <span className="prompt">$</span> curl -X POST
            https://api.mms.id/v1/messages \<br />
            &nbsp;&nbsp;-H &quot;Authorization: Bearer mms_live_…&quot; \<br />
            &nbsp;&nbsp;-d &apos;&#123;&quot;to&quot;:&quot;628123456789&quot;,&quot;type&quot;:&quot;text&quot;,&quot;body&quot;:&quot;Halo!&quot;&#125;&apos;
          </div>
          <div className="row" style={{ justifyContent: "center" }}>
            <Link to="/daftar" className="btn btn-primary">
              Mulai Sekarang
            </Link>
            <a href="#harga" className="btn">
              Lihat Harga
            </a>
          </div>
        </section>

        <section className="features">
          {FEATURES.map((f) => (
            <div className="card feature" key={f.t}>
              <h3>{f.t}</h3>
              <p>{f.d}</p>
            </div>
          ))}
        </section>

        <section className="section" id="harga">
          <div className="section-head">
            <h2>Pilih paket yang cocok untuk bisnis Anda</h2>
            <p>Upgrade atau ganti paket kapan saja.</p>
            <div className="toggle" style={{ marginTop: "1rem" }}>
              <button className={!yearly ? "on" : ""} onClick={() => setYearly(false)}>
                Bulanan
              </button>
              <button className={yearly ? "on" : ""} onClick={() => setYearly(true)}>
                Tahunan · hemat ~17%
              </button>
            </div>
          </div>

          <div className="pricing">
            {plans.map((p) => (
              <div
                className={"card plan" + (p.is_popular ? " popular" : "")}
                key={p.code}
              >
                {p.is_popular && <span className="plan-tag">Paling Populer</span>}
                <h3>{p.name}</h3>
                <p className="faint" style={{ fontSize: "0.85rem", margin: 0 }}>
                  {p.tagline}
                </p>
                <div className="price" style={{ marginTop: "0.75rem" }}>
                  {rupiah(yearly ? Math.round(p.price_idr_yearly / 12) : p.price_idr_monthly)}
                  <small>/bulan</small>
                </div>
                {yearly && (
                  <div className="hint">
                    Ditagih {rupiah(p.price_idr_yearly)} per tahun
                  </div>
                )}
                <ul>
                  <li>{p.max_broadcasts_per_month.toLocaleString("id-ID")} broadcast/bulan</li>
                  <li>{p.max_messages_per_month.toLocaleString("id-ID")} pesan/bulan</li>
                  <li>{p.max_wa_accounts} nomor WhatsApp</li>
                  <li>{p.max_members} member</li>
                  <li>Riwayat {p.history_retention_days} hari</li>
                  <li>{Math.round(p.max_storage_bytes / 1024 / 1024)} MB penyimpanan</li>
                </ul>
                <Link to="/daftar" className="btn btn-primary" style={{ width: "100%", justifyContent: "center" }}>
                  Pilih {p.name}
                </Link>
              </div>
            ))}
            {plans.length === 0 && (
              <p className="faint">
                Tidak bisa memuat daftar paket — pastikan API berjalan di port 8080.
              </p>
            )}
          </div>
        </section>

        <Calculator plans={plans} rates={rates} yearly={yearly} />

        <section className="section" id="faq">
          <div className="section-head">
            <h2>Pertanyaan umum</h2>
          </div>
          {FAQ.map((f) => (
            <details className="faq-item" key={f.q}>
              <summary>{f.q}</summary>
              <p>{f.a}</p>
            </details>
          ))}
        </section>
      </div>

      <footer>
        MMS — Platform WhatsApp Business API. Dibangun dengan Rust &amp; React.
      </footer>
    </>
  );
}

/**
 * Pricing calculator. Mirrors the server-side /public/estimate maths so the
 * page stays responsive while dragging, but the same rules are enforced
 * server-side for any quote that matters.
 */
function Calculator({
  plans,
  rates,
  yearly,
}: {
  plans: Plan[];
  rates: MetaRate[];
  yearly: boolean;
}) {
  const [planCode, setPlanCode] = useState("starter");
  const [marketing, setMarketing] = useState(0);
  const [utility, setUtility] = useState(0);
  const [service, setService] = useState(1000);
  const [numbers, setNumbers] = useState(1);

  useEffect(() => {
    if (plans.length && !plans.some((p) => p.code === planCode)) {
      setPlanCode(plans[0].code);
    }
  }, [plans, planCode]);

  const plan = plans.find((p) => p.code === planCode);

  const result = useMemo(() => {
    const rateOf = (cat: string) =>
      rates.find((r) => r.category === cat) ?? {
        rate_idr: 0,
        free_tier_per_number_per_month: 0,
      };

    const lines = (
      [
        ["Pesan marketing", "marketing", marketing],
        ["Pesan utility", "utility", utility],
        ["Service (balas pesan)", "service", service],
      ] as const
    ).map(([label, cat, volume]) => {
      const r = rateOf(cat);
      const free = Math.min(
        volume,
        r.free_tier_per_number_per_month * Math.max(1, numbers),
      );
      const billable = Math.max(0, volume - free);
      return { label, volume, free, billable, cost: billable * r.rate_idr };
    });

    const metaTotal = lines.reduce((s, l) => s + l.cost, 0);
    const platform = plan
      ? yearly
        ? plan.price_idr_yearly / 12
        : plan.price_idr_monthly
      : 0;
    return { lines, metaTotal, platform, total: platform + metaTotal };
  }, [rates, marketing, utility, service, numbers, plan, yearly]);

  return (
    <section className="section">
      <div className="section-head">
        <h2>Hitung kebutuhan Anda</h2>
        <p>
          Gabungkan biaya paket MMS dengan estimasi tarif Meta berdasarkan volume
          pesan Anda.
        </p>
      </div>

      <div className="calc">
        <div className="card">
          <div className="calc-row">
            <label className="label">Paket</label>
            <select
              className="select"
              value={planCode}
              onChange={(e) => setPlanCode(e.target.value)}
            >
              {plans.map((p) => (
                <option key={p.code} value={p.code}>
                  {p.name} — {rupiah(yearly ? Math.round(p.price_idr_yearly / 12) : p.price_idr_monthly)}/bulan
                </option>
              ))}
            </select>
          </div>

          <Slider
            label="Pesan marketing"
            hint="Broadcast dan promosi"
            value={marketing}
            max={50000}
            step={500}
            onChange={setMarketing}
          />
          <Slider
            label="Pesan utility"
            hint="Order, pembayaran, notifikasi"
            value={utility}
            max={50000}
            step={500}
            onChange={setUtility}
          />
          <Slider
            label="Service (balas pesan)"
            hint="Ada kuota gratis per nomor setiap bulan"
            value={service}
            max={50000}
            step={500}
            onChange={setService}
          />
          <Slider
            label="Jumlah nomor WhatsApp"
            hint="Kuota service gratis dihitung per nomor"
            value={numbers}
            max={20}
            step={1}
            onChange={setNumbers}
          />
        </div>

        <div className="card">
          <h3 style={{ fontSize: "0.8rem", textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--text-faint)" }}>
            Ringkasan estimasi bulanan
          </h3>
          <div className="total-line">
            <span>Biaya platform {plan?.name ?? ""}</span>
            <span className="calc-val">{rupiah(Math.round(result.platform))}</span>
          </div>
          {result.lines.map((l) => (
            <div className="total-line" key={l.label}>
              <span>
                {l.label}
                {l.free > 0 && (
                  <span className="faint"> · {l.free.toLocaleString("id-ID")} gratis</span>
                )}
              </span>
              <span className="calc-val">{rupiah(Math.round(l.cost))}</span>
            </div>
          ))}
          <div className="total-line grand">
            <span>Total estimasi / bulan</span>
            <span className="calc-val">{rupiah(Math.round(result.total))}</span>
          </div>
          <p className="hint">
            Biaya pesan ditagih langsung oleh Meta melalui metode pembayaran
            WhatsApp Business Anda. Estimasi belum termasuk PPN.
          </p>
        </div>
      </div>
    </section>
  );
}

function Slider({
  label,
  hint,
  value,
  max,
  step,
  onChange,
}: {
  label: string;
  hint: string;
  value: number;
  max: number;
  step: number;
  onChange: (n: number) => void;
}) {
  return (
    <div className="calc-row">
      <div className="between">
        <label className="label" style={{ marginBottom: 0 }}>
          {label}
        </label>
        <span className="calc-val">{value.toLocaleString("id-ID")}</span>
      </div>
      <input
        type="range"
        min={0}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <div className="hint">{hint}</div>
    </div>
  );
}

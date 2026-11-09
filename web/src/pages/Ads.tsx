import { useEffect, useState } from "react";
import { api, waktu } from "../api";
import { Empty, Err, PageHead } from "../components/bits";
import { Stat } from "../components/Chart";

type Summary = { referrals: number; contacts: number; ads: number; converted: number };
type Ad = {
  source_id: string | null;
  headline: string | null;
  source_url: string | null;
  source_type: string | null;
  leads: number;
  first_seen: string;
  last_seen: string;
};

export function Ads() {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [ads, setAds] = useState<Ad[]>([]);
  const [err, setErr] = useState("");

  useEffect(() => {
    void api
      .get<{ summary: Summary; ads: Ad[] }>("/api/ads/overview")
      .then((r) => {
        setSummary(r.summary);
        setAds(r.ads);
      })
      .catch((e) => setErr(e instanceof Error ? e.message : "Gagal memuat data iklan."));
  }, []);

  const rate =
    summary && summary.contacts > 0
      ? `${((summary.converted / summary.contacts) * 100).toFixed(1)}%`
      : "—";

  return (
    <div className="viz-root">
      <PageHead
        title="Ads"
        sub="Lead dari iklan Click-to-WhatsApp, dikelompokkan per iklan."
      />
      <Err>{err}</Err>

      <div
        className="grid"
        style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px,1fr))", marginBottom: "1.25rem" }}
      >
        <Stat label="Klik iklan" value={(summary?.referrals ?? 0).toLocaleString("id-ID")} sub="total referral" />
        <Stat label="Kontak dari iklan" value={(summary?.contacts ?? 0).toLocaleString("id-ID")} />
        <Stat label="Iklan aktif" value={(summary?.ads ?? 0).toLocaleString("id-ID")} />
        <Stat label="Jadi pelanggan" value={rate} sub={`${summary?.converted ?? 0} deal menang`} />
      </div>

      {ads.length === 0 ? (
        <Empty>
          Belum ada lead dari iklan. Attribution terisi otomatis saat pelanggan membuka
          chat dari iklan Click-to-WhatsApp.
        </Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <table>
            <thead>
              <tr>
                <th>Iklan</th>
                <th>Tipe</th>
                <th style={{ textAlign: "right" }}>Lead</th>
                <th>Pertama</th>
                <th>Terakhir</th>
              </tr>
            </thead>
            <tbody>
              {ads.map((a) => (
                <tr key={a.source_id ?? "unknown"}>
                  <td>
                    <div style={{ fontWeight: 600 }}>{a.headline ?? "(tanpa judul)"}</div>
                    <div className="faint mono" style={{ fontSize: "0.76rem" }}>
                      {a.source_id ?? "—"}
                    </div>
                  </td>
                  <td>
                    <span className="badge">{a.source_type ?? "—"}</span>
                  </td>
                  <td className="mono" style={{ textAlign: "right" }}>
                    {a.leads.toLocaleString("id-ID")}
                  </td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>{waktu(a.first_seen)}</td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>{waktu(a.last_seen)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

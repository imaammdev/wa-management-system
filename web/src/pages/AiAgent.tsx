import { useEffect, useState } from "react";
import { api, waktu, type WaAccount } from "../api";
import { Empty, Err, Ok, PageHead, StatusBadge } from "../components/bits";

type Agent = {
  id: string;
  wa_account_id: string | null;
  name: string;
  model: string;
  system_prompt: string;
  handoff_rules: { on_keyword?: string[]; after_turns?: number; on_intent?: string[] };
  enabled: boolean;
  fallback_message: string;
};

type Run = {
  id: string;
  model: string;
  status: string;
  intent: string | null;
  handoff_reason: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  latency_ms: number | null;
  error: string | null;
  created_at: string;
};

type ModelOpt = { id: string; label: string };

const BLANK: Agent = {
  id: "",
  wa_account_id: null,
  name: "Asisten CS",
  model: "claude-opus-5-5",
  system_prompt:
    "Jawab pertanyaan umum tentang produk, jam operasional, dan pengiriman.\nJangan menjanjikan diskon atau tanggal pengiriman yang pasti.",
  handoff_rules: { on_keyword: ["manusia", "komplain", "refund"], after_turns: 6 },
  enabled: false,
  fallback_message: "Mohon tunggu sebentar, tim kami akan segera membantu.",
};

export function AiAgent() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [numbers, setNumbers] = useState<WaAccount[]>([]);
  const [models, setModels] = useState<ModelOpt[]>([]);
  const [runs, setRuns] = useState<Run[]>([]);
  const [configured, setConfigured] = useState(true);
  const [draft, setDraft] = useState<Agent | null>(null);
  const [err, setErr] = useState("");
  const [ok, setOk] = useState("");

  const load = async () => {
    try {
      const [a, n, m, r] = await Promise.all([
        api.get<{ data: Agent[]; ai_configured: boolean }>("/api/ai/agents"),
        api.get<{ data: WaAccount[] }>("/api/whatsapp-accounts"),
        api.get<{ data: ModelOpt[] }>("/api/ai/models"),
        api.get<{ data: Run[] }>("/api/ai/runs?limit=25"),
      ]);
      setAgents(a.data);
      setConfigured(a.ai_configured);
      setNumbers(n.data);
      setModels(m.data);
      setRuns(r.data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat AI Agent.");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const save = async () => {
    if (!draft) return;
    setErr("");
    setOk("");
    try {
      await api.post("/api/ai/agents", {
        ...draft,
        id: draft.id || undefined,
      });
      setOk("Agent disimpan.");
      setDraft(null);
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal menyimpan agent.");
    }
  };

  const remove = async (id: string) => {
    if (!confirm("Hapus agent ini?")) return;
    await api.del(`/api/ai/agents/${id}`);
    await load();
  };

  const kw = (draft?.handoff_rules.on_keyword ?? []).join(", ");

  return (
    <>
      <PageHead
        title="AI Agent"
        sub="Balas pertanyaan umum otomatis, lalu serahkan ke tim saat pelanggan butuh manusia."
        action={
          !draft && (
            <button className="btn btn-primary" onClick={() => setDraft({ ...BLANK })}>
              + Agent baru
            </button>
          )
        }
      />
      <Err>{err}</Err>
      <Ok>{ok}</Ok>

      {!configured && (
        <div className="alert">
          <strong>ANTHROPIC_API_KEY belum diset.</strong> Agent bisa dikonfigurasi dan
          disimpan, tetapi worker akan melewati balasan otomatis sampai kunci API
          tersedia di environment.
        </div>
      )}

      {draft && (
        <div className="card" style={{ marginBottom: "1.5rem" }}>
          <h3 style={{ fontSize: "1rem" }}>{draft.id ? "Ubah agent" : "Agent baru"}</h3>
          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(260px,1fr))" }}>
            <div>
              <div className="field">
                <label className="label">Nama</label>
                <input
                  className="input"
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                />
              </div>
              <div className="field">
                <label className="label">Nomor WhatsApp</label>
                <select
                  className="select"
                  value={draft.wa_account_id ?? ""}
                  onChange={(e) =>
                    setDraft({ ...draft, wa_account_id: e.target.value || null })
                  }
                >
                  <option value="">— pilih nomor —</option>
                  {numbers.map((n) => (
                    <option key={n.id} value={n.id}>
                      {n.label}
                    </option>
                  ))}
                </select>
                <div className="hint">Satu agent per nomor.</div>
              </div>
              <div className="field">
                <label className="label">Model</label>
                <select
                  className="select"
                  value={draft.model}
                  onChange={(e) => setDraft({ ...draft, model: e.target.value })}
                >
                  {models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label}
                    </option>
                  ))}
                </select>
              </div>
              <label className="row" style={{ fontSize: "0.9rem" }}>
                <input
                  type="checkbox"
                  checked={draft.enabled}
                  onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
                />
                Aktifkan agent
              </label>
            </div>

            <div>
              <div className="field">
                <label className="label">Instruksi</label>
                <textarea
                  className="textarea"
                  rows={5}
                  value={draft.system_prompt}
                  onChange={(e) => setDraft({ ...draft, system_prompt: e.target.value })}
                />
                <div className="hint">
                  Aturan dasar (bahasa, nada, larangan mengarang) sudah ditambahkan otomatis.
                </div>
              </div>
              <div className="field">
                <label className="label">Serahkan ke manusia jika pesan memuat</label>
                <input
                  className="input"
                  value={kw}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      handoff_rules: {
                        ...draft.handoff_rules,
                        on_keyword: e.target.value
                          .split(",")
                          .map((s) => s.trim())
                          .filter(Boolean),
                      },
                    })
                  }
                  placeholder="manusia, komplain, refund"
                />
              </div>
              <div className="field">
                <label className="label">Serahkan setelah berapa giliran</label>
                <input
                  className="input"
                  type="number"
                  min={1}
                  value={draft.handoff_rules.after_turns ?? 6}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      handoff_rules: {
                        ...draft.handoff_rules,
                        after_turns: Number(e.target.value),
                      },
                    })
                  }
                />
              </div>
            </div>
          </div>

          <div className="row" style={{ marginTop: "0.5rem" }}>
            <button className="btn btn-primary" onClick={() => void save()}>
              Simpan
            </button>
            <button className="btn" onClick={() => setDraft(null)}>
              Batal
            </button>
          </div>
        </div>
      )}

      {agents.length === 0 && !draft ? (
        <Empty>Belum ada AI Agent.</Empty>
      ) : (
        <div className="stack" style={{ marginBottom: "1.5rem" }}>
          {agents.map((a) => (
            <div className="card" key={a.id}>
              <div className="between wrap">
                <div style={{ minWidth: 0 }}>
                  <div className="row wrap">
                    <strong>{a.name}</strong>
                    <StatusBadge status={a.enabled ? "active" : "disabled"} />
                    <span className="badge mono">{a.model}</span>
                    <span className="badge">
                      {numbers.find((n) => n.id === a.wa_account_id)?.label ?? "tanpa nomor"}
                    </span>
                  </div>
                  <p className="muted" style={{ fontSize: "0.86rem", margin: "0.4rem 0 0" }}>
                    {a.system_prompt.slice(0, 160)}
                    {a.system_prompt.length > 160 ? "…" : ""}
                  </p>
                </div>
                <div className="row">
                  <button className="btn btn-sm" onClick={() => setDraft(a)}>
                    Ubah
                  </button>
                  <button className="btn btn-sm btn-danger" onClick={() => void remove(a.id)}>
                    Hapus
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      <h3 style={{ fontSize: "1rem" }}>Riwayat jalan</h3>
      {runs.length === 0 ? (
        <Empty>Belum ada balasan otomatis.</Empty>
      ) : (
        <div className="card" style={{ padding: 0, overflow: "hidden" }}>
          <table>
            <thead>
              <tr>
                <th>Waktu</th>
                <th>Status</th>
                <th>Intent</th>
                <th style={{ textAlign: "right" }}>Token</th>
                <th style={{ textAlign: "right" }}>Latensi</th>
                <th>Catatan</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((r) => (
                <tr key={r.id}>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>{waktu(r.created_at)}</td>
                  <td>
                    <StatusBadge status={r.status === "ok" ? "delivered" : r.status} />
                  </td>
                  <td className="mono" style={{ fontSize: "0.82rem" }}>{r.intent ?? "—"}</td>
                  <td className="mono faint" style={{ textAlign: "right" }}>
                    {r.input_tokens !== null ? `${r.input_tokens}/${r.output_tokens}` : "—"}
                  </td>
                  <td className="mono faint" style={{ textAlign: "right" }}>
                    {r.latency_ms !== null ? `${r.latency_ms} ms` : "—"}
                  </td>
                  <td className="faint" style={{ fontSize: "0.8rem" }}>
                    {r.error ?? r.handoff_reason ?? "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

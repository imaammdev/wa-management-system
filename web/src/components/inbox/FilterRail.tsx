import type { InboxCounts, Label, Member, WaAccount } from "../../api";
import { PRIORITAS } from "../../api";

export type Filters = {
  status: string | null;
  bucket: string | null;
  priority: string | null;
  assigned_to: string | null;
  label_id: string | null;
  wa_account_id: string | null;
};

export const EMPTY_FILTERS: Filters = {
  status: null,
  bucket: null,
  priority: null,
  assigned_to: null,
  label_id: null,
  wa_account_id: null,
};

/** True when nothing but the one key under test is set — keeps the rail's
 *  highlight honest when several filters are combined. */
function active(f: Filters, key: keyof Filters, value: string | null) {
  return f[key] === value;
}

export function FilterRail({
  counts,
  filters,
  onChange,
  numbers,
  members,
  labels,
  onNewLabel,
}: {
  counts: InboxCounts | null;
  filters: Filters;
  onChange: (f: Filters) => void;
  numbers: WaAccount[];
  members: Member[];
  labels: Label[];
  onNewLabel: () => void;
}) {
  const c = counts;
  const set = (patch: Partial<Filters>) => onChange({ ...filters, ...patch });

  // Picking a status clears the derived bucket and vice versa: they are two
  // views of the same column and would otherwise fight each other.
  const pickStatus = (status: string | null) =>
    set({ status, bucket: null });
  const pickBucket = (bucket: string | null) =>
    set({ bucket, status: null });

  return (
    <aside className="rail">
      <div className="between" style={{ marginBottom: "0.5rem" }}>
        <strong style={{ fontSize: "0.95rem" }}>Inbox</strong>
      </div>

      <select
        className="select"
        style={{ fontSize: "0.82rem", padding: "0.4rem 0.5rem" }}
        value={filters.wa_account_id ?? ""}
        onChange={(e) => set({ wa_account_id: e.target.value || null })}
      >
        <option value="">Semua nomor</option>
        {numbers.map((n) => (
          <option key={n.id} value={n.id}>
            {n.label}
            {n.display_phone_number ? ` (${n.display_phone_number})` : ""}
          </option>
        ))}
      </select>

      <h4>Status</h4>
      <Item
        label="Semua"
        count={c?.total}
        on={!filters.status && !filters.bucket}
        onClick={() => pickStatus(null)}
      />
      <Item
        label="Belum dibaca"
        count={c?.unread}
        on={active(filters, "bucket", "unread")}
        onClick={() => pickBucket("unread")}
      />
      <Item
        label="Belum dibalas"
        count={c?.awaiting}
        on={active(filters, "bucket", "awaiting")}
        onClick={() => pickBucket("awaiting")}
      />
      <Item
        label="Terbuka"
        count={c?.open}
        on={active(filters, "status", "open")}
        onClick={() => pickStatus("open")}
      />
      <Item
        label="Menunggu"
        count={c?.pending}
        on={active(filters, "status", "pending")}
        onClick={() => pickStatus("pending")}
      />
      <Item
        label="Ditahan"
        count={c?.held}
        on={active(filters, "status", "held")}
        onClick={() => pickStatus("held")}
      />
      <Item
        label="Selesai"
        count={c?.closed}
        on={active(filters, "status", "closed")}
        onClick={() => pickStatus("closed")}
      />

      <h4>Penerima tugas</h4>
      <Item
        label="Siapa saja"
        count={c?.total}
        on={!filters.assigned_to}
        onClick={() => set({ assigned_to: null })}
      />
      <Item
        label="Saya"
        count={c?.mine}
        on={active(filters, "assigned_to", "me")}
        onClick={() => set({ assigned_to: "me" })}
      />
      <Item
        label="Belum ditugaskan"
        count={c?.unassigned}
        on={active(filters, "assigned_to", "none")}
        onClick={() => set({ assigned_to: "none" })}
      />

      {members.length > 0 && (
        <>
          <h4>Anggota tim</h4>
          {members.map((m) => (
            <button
              key={m.id}
              className={"rail-item" + (filters.assigned_to === m.id ? " on" : "")}
              onClick={() => set({ assigned_to: m.id })}
            >
              <span className="avatar" style={{ width: 22, height: 22, fontSize: "0.6rem" }}>
                {m.name
                  .split(/\s+/)
                  .slice(0, 2)
                  .map((w) => w[0])
                  .join("")
                  .toUpperCase()}
              </span>
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {m.name}
                <br />
                <span className="faint" style={{ fontSize: "0.7rem" }}>
                  {m.role}
                </span>
              </span>
            </button>
          ))}
        </>
      )}

      <h4>Prioritas</h4>
      <Item
        label="Apa saja"
        on={!filters.priority}
        onClick={() => set({ priority: null })}
      />
      {Object.entries(PRIORITAS).map(([key, p]) => (
        <button
          key={key}
          className={"rail-item" + (filters.priority === key ? " on" : "")}
          onClick={() => set({ priority: key })}
        >
          <span className="dot" style={{ background: p.color }} />
          {p.label}
          {key === "urgent" && c ? <span className="count">{c.urgent}</span> : null}
        </button>
      ))}

      <h4 className="between" style={{ display: "flex" }}>
        Label
        <button
          className="rail-item"
          style={{ width: "auto", padding: "0 0.2rem" }}
          onClick={onNewLabel}
          title="Tambah label"
        >
          +
        </button>
      </h4>
      <Item
        label="Semua label"
        on={!filters.label_id}
        onClick={() => set({ label_id: null })}
      />
      {labels.map((l) => (
        <button
          key={l.id}
          className={"rail-item" + (filters.label_id === l.id ? " on" : "")}
          onClick={() => set({ label_id: l.id })}
        >
          <span className="dot" style={{ background: l.color }} />
          {l.name}
        </button>
      ))}
    </aside>
  );
}

function Item({
  label,
  count,
  on,
  onClick,
}: {
  label: string;
  count?: number;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button className={"rail-item" + (on ? " on" : "")} onClick={onClick}>
      {label}
      {count !== undefined && <span className="count">{count}</span>}
    </button>
  );
}

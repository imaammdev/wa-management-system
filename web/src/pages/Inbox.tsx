import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  type CannedResponse,
  type Conversation,
  type InboxCounts,
  type Label,
  type Member,
  type Message,
  type WaAccount,
} from "../api";
import { ConversationList } from "../components/inbox/ConversationList";
import { CustomerPanel } from "../components/inbox/CustomerPanel";
import { EMPTY_FILTERS, FilterRail, type Filters } from "../components/inbox/FilterRail";
import { Thread } from "../components/inbox/Thread";

/** Builds the querystring for the conversation list from the rail's state. */
function toQuery(f: Filters, search: string, tab: "open" | "urgent") {
  const q = new URLSearchParams();
  // The "Mendesak" tab is a shortcut over the same filter machinery.
  if (tab === "urgent") q.set("bucket", "urgent");
  else {
    if (f.status) q.set("status", f.status);
    if (f.bucket) q.set("bucket", f.bucket);
  }
  if (f.priority) q.set("priority", f.priority);
  if (f.assigned_to) q.set("assigned_to", f.assigned_to);
  if (f.label_id) q.set("label_id", f.label_id);
  if (f.wa_account_id) q.set("wa_account_id", f.wa_account_id);
  if (search.trim()) q.set("search", search.trim());
  const s = q.toString();
  return s ? `?${s}` : "";
}

export function Inbox() {
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS);
  const [tab, setTab] = useState<"open" | "urgent">("open");
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");

  const [rows, setRows] = useState<Conversation[]>([]);
  const [counts, setCounts] = useState<InboxCounts | null>(null);
  const [numbers, setNumbers] = useState<WaAccount[]>([]);
  const [members, setMembers] = useState<Member[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [canned, setCanned] = useState<CannedResponse[]>([]);

  const [active, setActive] = useState<Conversation | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [cursor, setCursor] = useState(0);
  const [showPanel, setShowPanel] = useState(true);
  const [err, setErr] = useState("");

  // Typing in search shouldn't fire a request per keystroke.
  useEffect(() => {
    const t = setTimeout(() => setDebounced(search), 250);
    return () => clearTimeout(t);
  }, [search]);

  // Reference data, loaded once.
  useEffect(() => {
    void Promise.all([
      api.get<{ data: WaAccount[] }>("/api/whatsapp-accounts"),
      api.get<{ data: Member[] }>("/api/organizations/current/members"),
      api.get<{ data: Label[] }>("/api/contacts/labels"),
      api.get<{ data: CannedResponse[] }>("/api/inbox/canned-responses"),
    ])
      .then(([n, m, l, c]) => {
        setNumbers(n.data);
        setMembers(m.data);
        setLabels(l.data);
        setCanned(c.data);
      })
      .catch((e) => setErr(e instanceof Error ? e.message : "Gagal memuat data."));
  }, []);

  const loadList = useCallback(async () => {
    try {
      const qs = toQuery(filters, debounced, tab);
      const countQs = filters.wa_account_id
        ? `?wa_account_id=${filters.wa_account_id}`
        : "";
      const [l, c] = await Promise.all([
        api.get<{ data: Conversation[] }>(`/api/inbox/conversations${qs}`),
        api.get<{ counts: InboxCounts }>(`/api/inbox/counts${countQs}`),
      ]);
      setRows(l.data);
      setCounts(c.counts);
      setErr("");
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memuat percakapan.");
    }
  }, [filters, debounced, tab]);

  useEffect(() => {
    void loadList();
  }, [loadList]);

  const loadMessages = useCallback(async (id: string) => {
    const r = await api.get<{ data: Message[] }>(
      `/api/inbox/conversations/${id}/messages?limit=100`,
    );
    setMessages(r.data.slice().reverse());
    await api.post(`/api/inbox/conversations/${id}/read`).catch(() => {});
  }, []);

  const open = useCallback(
    async (c: Conversation) => {
      setActive(c);
      setMessages([]);
      try {
        await loadMessages(c.id);
        await loadList();
      } catch (e) {
        setErr(e instanceof Error ? e.message : "Gagal memuat pesan.");
      }
    },
    [loadMessages, loadList],
  );

  // Poll for new traffic; cheap enough at this cadence and avoids a socket.
  const activeRef = useRef<Conversation | null>(null);
  activeRef.current = active;
  useEffect(() => {
    const t = setInterval(() => {
      void loadList();
      const a = activeRef.current;
      if (a) void loadMessages(a.id).catch(() => {});
    }, 6000);
    return () => clearInterval(t);
  }, [loadList, loadMessages]);

  // ↑/↓ move the cursor, Enter opens — but never while typing in a field.
  //
  // The handler reads rows/cursor through refs and is bound exactly once.
  // Depending on them directly would tear the listener down and rebuild it on
  // every move, and a key pressed in that gap is silently dropped — which is
  // precisely what fast repeat does.
  const rowsRef = useRef<Conversation[]>([]);
  const cursorRef = useRef(0);
  const openRef = useRef(open);
  rowsRef.current = rows;
  cursorRef.current = cursor;
  openRef.current = open;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName)) return;
      const n = rowsRef.current.length;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setCursor((i) => Math.min(i + 1, Math.max(n - 1, 0)));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setCursor((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter") {
        const row = rowsRef.current[cursorRef.current];
        if (row) {
          e.preventDefault();
          void openRef.current(row);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      await loadList();
      if (active) {
        const fresh = await api.get<{ conversation: Conversation }>(
          `/api/inbox/conversations/${active.id}`,
        );
        setActive((a) => (a ? { ...a, ...fresh.conversation } : a));
      }
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Gagal memperbarui.");
    }
  };

  const newLabel = async () => {
    const name = prompt("Nama label baru?");
    if (!name) return;
    await api.post("/api/contacts/labels", { name });
    const l = await api.get<{ data: Label[] }>("/api/contacts/labels");
    setLabels(l.data);
  };

  const shellClass = useMemo(
    () => "inbox-shell" + (active && showPanel ? " show-panel" : " no-panel"),
    [active, showPanel],
  );

  return (
    <div className="inbox-page">
      {err && (
        <div className="alert" style={{ margin: "0.5rem" }}>
          {err}
        </div>
      )}
      <div className={shellClass}>
        <FilterRail
          counts={counts}
          filters={filters}
          onChange={(f) => {
            setFilters(f);
            setCursor(0);
          }}
          numbers={numbers}
          members={members}
          labels={labels}
          onNewLabel={() => void newLabel()}
        />

        <ConversationList
          rows={rows}
          counts={counts}
          activeId={active?.id ?? null}
          cursor={cursor}
          tab={tab}
          search={search}
          onTab={(t) => {
            setTab(t);
            setCursor(0);
          }}
          onSearch={setSearch}
          onOpen={(c) => void open(c)}
        />

        {active ? (
          <Thread
            conv={active}
            messages={messages}
            members={members}
            canned={canned}
            onReply={async (text) => {
              await api.post(`/api/inbox/conversations/${active.id}/messages`, {
                type: "text",
                body: text,
              });
              await loadMessages(active.id);
              await loadList();
            }}
            onStatus={(s) =>
              void act(() =>
                api.post(`/api/inbox/conversations/${active.id}/status`, { status: s }),
              )
            }
            onAssign={(u) =>
              void act(() =>
                api.post(`/api/inbox/conversations/${active.id}/assign`, { user_id: u }),
              )
            }
            onPriority={(p) =>
              void act(() =>
                api.post(`/api/inbox/conversations/${active.id}/priority`, { priority: p }),
              )
            }
            onTogglePanel={() => setShowPanel((v) => !v)}
          />
        ) : (
          <div className="thread-col">
            <p className="faint" style={{ margin: "auto" }}>
              Pilih percakapan, atau tekan <kbd>Enter</kbd> pada baris yang disorot.
            </p>
          </div>
        )}

        {active && showPanel && (
          <CustomerPanel
            conv={active}
            labels={labels}
            onClose={() => setShowPanel(false)}
            onChanged={() => void loadList()}
          />
        )}
      </div>
    </div>
  );
}

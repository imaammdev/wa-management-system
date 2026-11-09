/**
 * Small inline-SVG chart kit.
 *
 * Colors come from the validated categorical palette (slots 1–4), declared as
 * CSS custom properties on `.viz-root` so light/dark swap in one place rather
 * than being branched in JS. Validated with the palette checker against both
 * surfaces: worst adjacent CVD ΔE 9.1 light / 8.4 dark.
 */
import { useId, useState } from "react";

export const SERIES = ["var(--series-1)", "var(--series-2)", "var(--series-3)", "var(--series-4)"];

export type Point = { label: string; values: number[] };

/** Nice-ish axis maximum, so the top gridline is a round number. */
function niceMax(v: number) {
  if (v <= 0) return 1;
  const mag = 10 ** Math.floor(Math.log10(v));
  return Math.ceil(v / mag) * mag;
}

export function LineChart({
  points,
  names,
  height = 200,
}: {
  points: Point[];
  names: string[];
  height?: number;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const clip = useId();

  const W = 760;
  const H = height;
  const pad = { t: 10, r: 12, b: 24, l: 40 };
  const iw = W - pad.l - pad.r;
  const ih = H - pad.t - pad.b;

  const max = niceMax(Math.max(1, ...points.flatMap((p) => p.values)));
  const x = (i: number) => pad.l + (points.length < 2 ? iw / 2 : (i / (points.length - 1)) * iw);
  const y = (v: number) => pad.t + ih - (v / max) * ih;

  const path = (s: number) =>
    points.map((p, i) => `${i === 0 ? "M" : "L"}${x(i)},${y(p.values[s] ?? 0)}`).join(" ");

  const ticks = [0, 0.5, 1].map((f) => Math.round(max * f));

  return (
    <div style={{ position: "relative" }}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        width="100%"
        height={H}
        role="img"
        aria-label={`Grafik ${names.join(" dan ")} per hari`}
        onMouseLeave={() => setHover(null)}
      >
        <defs>
          <clipPath id={clip}>
            <rect x={pad.l} y={pad.t} width={iw} height={ih} />
          </clipPath>
        </defs>

        {/* recessive grid */}
        {ticks.map((t) => (
          <g key={t}>
            <line
              x1={pad.l}
              x2={W - pad.r}
              y1={y(t)}
              y2={y(t)}
              stroke="var(--viz-grid)"
              strokeWidth="1"
            />
            <text x={pad.l - 6} y={y(t) + 4} textAnchor="end" fontSize="10" fill="var(--viz-muted)">
              {t}
            </text>
          </g>
        ))}

        <g clipPath={`url(#${clip})`}>
          {names.map((_, s) => (
            <path
              key={s}
              d={path(s)}
              fill="none"
              stroke={SERIES[s]}
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
            />
          ))}
        </g>

        {/* crosshair */}
        {hover !== null && (
          <line
            x1={x(hover)}
            x2={x(hover)}
            y1={pad.t}
            y2={pad.t + ih}
            stroke="var(--viz-muted)"
            strokeWidth="1"
            strokeDasharray="3 3"
          />
        )}
        {hover !== null &&
          names.map((_, s) => (
            <circle
              key={s}
              cx={x(hover)}
              cy={y(points[hover].values[s] ?? 0)}
              r="4.5"
              fill={SERIES[s]}
              stroke="var(--viz-surface)"
              strokeWidth="2"
            />
          ))}

        {/* x labels: first, middle, last only — no label soup */}
        {points.length > 0 &&
          [0, Math.floor(points.length / 2), points.length - 1]
            .filter((v, i, a) => a.indexOf(v) === i)
            .map((i) => (
              <text
                key={i}
                x={x(i)}
                y={H - 6}
                textAnchor={i === 0 ? "start" : i === points.length - 1 ? "end" : "middle"}
                fontSize="10"
                fill="var(--viz-muted)"
              >
                {points[i].label}
              </text>
            ))}

        {/* hit targets, wider than the marks */}
        {points.map((_, i) => (
          <rect
            key={i}
            x={x(i) - iw / Math.max(points.length, 1) / 2}
            y={pad.t}
            width={iw / Math.max(points.length, 1)}
            height={ih}
            fill="transparent"
            onMouseEnter={() => setHover(i)}
          />
        ))}
      </svg>

      {hover !== null && (
        <div
          style={{
            position: "absolute",
            top: 0,
            left: `${(x(hover) / W) * 100}%`,
            transform: "translateX(-50%)",
            background: "var(--bg-elev-2)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            padding: "0.4rem 0.6rem",
            fontSize: "0.78rem",
            pointerEvents: "none",
            whiteSpace: "nowrap",
          }}
        >
          <strong>{points[hover].label}</strong>
          {names.map((n, s) => (
            <div key={n} className="row" style={{ gap: "0.35rem" }}>
              <span
                style={{ width: 8, height: 8, borderRadius: 2, background: SERIES[s] }}
              />
              {n}: <strong>{points[hover].values[s] ?? 0}</strong>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function Legend({ names }: { names: string[] }) {
  return (
    <div className="row wrap" style={{ gap: "0.9rem", fontSize: "0.8rem" }}>
      {names.map((n, i) => (
        <span key={n} className="row" style={{ gap: "0.35rem" }}>
          <span style={{ width: 10, height: 10, borderRadius: 3, background: SERIES[i] }} />
          <span className="muted">{n}</span>
        </span>
      ))}
    </div>
  );
}

/**
 * Segmented bar with direct value labels. The labels are not decoration —
 * two of the light-mode slots sit under 3:1 on white, so the palette's relief
 * rule requires them.
 */
export function SegmentBar({
  segments,
}: {
  segments: Array<{ name: string; value: number }>;
}) {
  const total = segments.reduce((s, x) => s + x.value, 0);
  if (total === 0) return <p className="faint">Belum ada data.</p>;

  return (
    <>
      <div style={{ display: "flex", gap: 2, height: 14, marginBottom: "0.6rem" }}>
        {segments.map((s, i) =>
          s.value === 0 ? null : (
            <div
              key={s.name}
              title={`${s.name}: ${s.value}`}
              style={{
                width: `${(s.value / total) * 100}%`,
                background: SERIES[i % SERIES.length],
                borderRadius: i === 0 ? "7px 2px 2px 7px" : "2px",
              }}
            />
          ),
        )}
      </div>
      <table>
        <tbody>
          {segments.map((s, i) => (
            <tr key={s.name}>
              <td style={{ padding: "0.3rem 0" }}>
                <span className="row" style={{ gap: "0.4rem" }}>
                  <span
                    style={{ width: 10, height: 10, borderRadius: 3, background: SERIES[i % SERIES.length] }}
                  />
                  {s.name}
                </span>
              </td>
              <td style={{ padding: "0.3rem 0", textAlign: "right" }} className="mono">
                {s.value.toLocaleString("id-ID")}
              </td>
              <td
                style={{ padding: "0.3rem 0", textAlign: "right", width: "4rem" }}
                className="faint mono"
              >
                {Math.round((s.value / total) * 100)}%
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

/** A headline number is a better answer than a one-bar chart. */
export function Stat({
  label,
  value,
  sub,
}: {
  label: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="card">
      <div className="faint" style={{ fontSize: "0.74rem", textTransform: "uppercase", letterSpacing: "0.07em" }}>
        {label}
      </div>
      <div style={{ fontSize: "1.7rem", fontWeight: 800, letterSpacing: "-0.02em", marginTop: "0.2rem" }}>
        {value}
      </div>
      {sub && <div className="faint" style={{ fontSize: "0.78rem" }}>{sub}</div>}
    </div>
  );
}

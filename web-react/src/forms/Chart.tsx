// The wealth series, drawn by hand as an SVG path.
//
// Hand-drawn rather than charted by a library, for the same reason the ABI is
// hand-written: no dependency earns its place here. The projected years are
// dashed, because a solid line through them would claim a certainty the figures
// do not have.
//
// The chart is `role="img"` with a label, and the table below carries the same
// numbers — a picture nobody can read is not an answer.

import { useCallback, useMemo, useRef, useState } from "react";
import { euro } from "../format";
import type { ProjectionRow } from "../types";

interface ChartProps {
  series: ProjectionRow[];
  label: string;
  caption: string;
}

export function Chart({ series, label, caption }: ChartProps) {
  const W = 640;
  const H = 240;
  const PAD = { l: 8, r: 8, t: 12, b: 22 };

  const svgRef = useRef<SVGSVGElement>(null);
  const [hover, setHover] = useState<{ x: number; y: number; year: number; wealth: number } | null>(
    null,
  );

  const max = Math.max(...series.map((r) => r.wealth), 1);
  const min = Math.min(...series.map((r) => r.wealth), 0);
  const span = max - min || 1;

  const x = (i: number): number =>
    PAD.l + (i / Math.max(series.length - 1, 1)) * (W - PAD.l - PAD.r);
  const y = (v: number): number => H - PAD.b - ((v - min) / span) * (H - PAD.t - PAD.b);

  const lastEnacted = series.reduce((n, r, i) => (r.enacted ? i : n), 0);

  const path = useCallback(
    (from: number, to: number): string =>
      series
        .slice(from, to)
        .map((r, k) => `${k ? "L" : "M"}${x(from + k).toFixed(1)},${y(r.wealth).toFixed(1)}`)
        .join(" "),
    [series],
  );

  const areaPath = useMemo(() => {
    const line = series
      .map((r, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(r.wealth).toFixed(1)}`)
      .join(" ");
    const endX = x(series.length - 1).toFixed(1);
    const bottomY = H - PAD.b;
    return `${line} L${endX},${bottomY} L${PAD.l},${bottomY} Z`;
  }, [series]);

  const handleMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const mouseX = (e.clientX - rect.left) * (W / rect.width);
    const i = Math.min(
      series.length - 1,
      Math.max(0, Math.round(((mouseX - PAD.l) / (W - PAD.l - PAD.r)) * (series.length - 1))),
    );
    const r = series[i];
    setHover({ x: x(i), y: y(r.wealth), year: r.year, wealth: r.wealth });
  };

  const zero =
    min < 0 ? (
      <line
        className="axis"
        x1={PAD.l}
        x2={W - PAD.r}
        y1={y(0).toFixed(1)}
        y2={y(0).toFixed(1)}
      />
    ) : null;

  return (
    <figure onMouseLeave={() => setHover(null)}>
      <svg
        ref={svgRef}
        className="chart"
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label={label}
        onMouseMove={handleMove}
      >
        <line
          className="grid"
          x1={PAD.l}
          x2={W - PAD.r}
          y1={y(max).toFixed(1)}
          y2={y(max).toFixed(1)}
        />
        {zero}
        <path className="area" d={areaPath} />
        <path className="line" d={path(0, lastEnacted + 1)} />
        <path className="line projected" d={path(lastEnacted, series.length)} />
        {hover && (
          <line
            className="cursor"
            x1={hover.x.toFixed(1)}
            x2={hover.x.toFixed(1)}
            y1={PAD.t}
            y2={H - PAD.b}
          />
        )}
        <text x={PAD.l} y={H - 6}>
          {series[0].year}
        </text>
        <text x={W - PAD.r} y={H - 6} textAnchor="end">
          {series[series.length - 1].year}
        </text>
        <text x={PAD.l} y={(y(max) - 4).toFixed(1)}>
          {euro(max)}
        </text>
      </svg>
      {hover && (
        <div
          className={`chart-tooltip visible`}
          style={{
            left: `${(hover.x / W) * 100}%`,
            top: `${(hover.y / H) * 100}%`,
          }}
        >
          <strong>{hover.year}</strong> · {euro(hover.wealth)}
        </div>
      )}
      <figcaption>{caption}</figcaption>
    </figure>
  );
}

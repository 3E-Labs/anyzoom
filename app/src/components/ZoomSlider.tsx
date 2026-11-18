import { formatZoom, ZOOM_MAX, ZOOM_MIN, type View } from "../lib/api";

const TICKS = Array.from({ length: (ZOOM_MAX - ZOOM_MIN) * 4 + 1 }, (_, i) => ZOOM_MIN + i / 4);

/** Zoom as a ruler: a mark every quarter step, a number at every whole one. */
export function ZoomSlider({ view, onView, onReset }: { view: View; onView: (v: View) => void; onReset: () => void }) {
  const at = (zoom: number) => `${((zoom - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN)) * 100}%`;
  return (
    <div className="ruler">
      <div className="ruler-track">
        <div className="ruler-ticks" aria-hidden="true">
          {TICKS.map((t) => (
            <i key={t} className={Number.isInteger(t) ? "major" : ""} style={{ left: at(t) }} />
          ))}
          {TICKS.filter(Number.isInteger).map((t) => (
            <span key={t} style={{ left: at(t) }}>
              {t}×
            </span>
          ))}
        </div>
        <input
          type="range"
          aria-label="Zoom"
          min={ZOOM_MIN}
          max={ZOOM_MAX}
          step={0.01}
          value={view.zoom}
          onChange={(e) => onView({ ...view, zoom: Number(e.target.value) })}
          onDoubleClick={onReset}
        />
      </div>
      <div className="zoom-now" aria-live="off">
        {formatZoom(view.zoom)}
      </div>
    </div>
  );
}

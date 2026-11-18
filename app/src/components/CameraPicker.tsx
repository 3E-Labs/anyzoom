import type { Camera } from "../lib/api";

type Props = { cameras: Camera[]; selected: string | null; onSelect: (id: string | null) => void; onRefresh: () => void };

export function CameraPicker({ cameras, selected, onSelect, onRefresh }: Props) {
  const missing = selected !== null && !cameras.some((c) => c.id === selected);
  return (
    <>
      <select
        className="select"
        aria-label="Camera"
        value={selected ?? ""}
        onPointerDown={onRefresh}
        onFocus={onRefresh}
        onChange={(e) => onSelect(e.target.value === "" ? null : e.target.value)}
      >
        <option value="">{cameras.length ? `First available (${cameras[0].name})` : "No camera found"}</option>
        {cameras.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
        {missing && <option value={selected}>Previously chosen camera (not connected)</option>}
      </select>
    </>
  );
}

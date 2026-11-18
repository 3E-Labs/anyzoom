import { useEffect, useState } from "react";
import { ACTION_LABELS, eventToAccelerator, HOTKEY_ACTIONS, type HotkeyAction } from "../lib/api";

type Props = {
  hotkeys: Record<HotkeyAction, string>;
  errors: Partial<Record<HotkeyAction, string>>;
  onChange: (action: HotkeyAction, accelerator: string) => void;
};

/** Click a binding, then press the new keys. Esc cancels, Backspace clears. */
export function HotkeyList({ hotkeys, errors, onChange }: Props) {
  const [listening, setListening] = useState<HotkeyAction | null>(null);

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") return setListening(null);
      if (e.code === "Backspace" || e.code === "Delete") {
        onChange(listening, "");
        return setListening(null);
      }
      const accelerator = eventToAccelerator(e);
      if (accelerator) {
        onChange(listening, accelerator);
        setListening(null);
      }
    };
    const stop = () => setListening(null);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", stop);
    };
  }, [listening, onChange]);

  return (
    <ul className="keys">
      {HOTKEY_ACTIONS.map((action) => {
        const error = errors[action];
        const active = listening === action;
        return (
          <li key={action}>
            <div className="row">
              <span>{ACTION_LABELS[action]}</span>
              <button
                type="button"
                className={`keycap${active ? " listening" : ""}${error ? " broken" : ""}`}
                aria-label={`Change the keys for ${ACTION_LABELS[action]}`}
                onClick={() => setListening(active ? null : action)}
              >
                {active ? "Press keys…" : hotkeys[action] || "Not set"}
              </button>
            </div>
            {error && !active && <p className="error">Not working: {error}.</p>}
          </li>
        );
      })}
    </ul>
  );
}

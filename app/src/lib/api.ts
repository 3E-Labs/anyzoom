// Types and calls shared with the Rust side, plus the small pure helpers the UI needs.
import { invoke } from "@tauri-apps/api/core";

export const ZOOM_MIN = 1;
export const ZOOM_MAX = 4;

export type View = { zoom: number; pan_x: number; pan_y: number };
export type Rect = { x: number; y: number; w: number; h: number };
export type Output = "720p" | "1080p";
export type Camera = { id: string; name: string };

export const HOTKEY_ACTIONS = ["zoom_in", "zoom_out", "reset", "pan_left", "pan_right", "pan_up", "pan_down"] as const;
export type HotkeyAction = (typeof HOTKEY_ACTIONS)[number];

export type Settings = {
  camera_id: string | null;
  view: View;
  output: Output;
  hotkeys: Record<HotkeyAction, string>;
  start_with_windows: boolean;
};

export type Status = {
  camera_open: boolean;
  camera_error: string | null;
  consumer_active: boolean;
  src_size: [number, number] | null;
  fps: number;
  view: View;
  vcam_error: string | null;
};

export type AppState = {
  settings: Settings;
  status: Status;
  hotkey_errors: Partial<Record<HotkeyAction, string>>;
  version: string;
};

export type Preview = { jpeg_base64: string; crop: Rect; src: [number, number] };

export const api = {
  listCameras: () => invoke<Camera[]>("list_cameras"),
  getState: () => invoke<AppState>("get_state"),
  setCamera: (id: string | null) => invoke<void>("set_camera", { id }),
  setOutput: (output: Output) => invoke<void>("set_output", { output }),
  setView: (view: View) => invoke<void>("set_view", { view }),
  resetView: () => invoke<void>("reset_view"),
  setHotkey: (action: HotkeyAction, accelerator: string) => invoke<void>("set_hotkey", { action, accelerator }),
  setStartWithWindows: (on: boolean) => invoke<void>("set_start_with_windows", { on }),
  setPreviewWanted: (on: boolean) => invoke<void>("set_preview_wanted", { on }),
  getPreview: () => invoke<Preview | null>("get_preview"),
};

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function formatZoom(zoom: number): string {
  return `${zoom.toFixed(2)}×`;
}

/** New zoom after a mouse-wheel movement; rolling away from you zooms in. */
export function wheelToZoom(zoom: number, deltaY: number): number {
  if (!Number.isFinite(deltaY)) return zoom;
  return clamp(zoom * Math.exp(-deltaY * 0.0015), ZOOM_MIN, ZOOM_MAX);
}

/**
 * New view after dragging the crop frame. `dx` and `dy` are the distance
 * dragged as a fraction of the picture's width and height.
 */
export function dragToPan(view: View, dx: number, dy: number): View {
  const travel = (1 - 1 / view.zoom) / 2; // how far the frame's centre can move each way
  if (travel <= 0) return view;
  return {
    zoom: view.zoom,
    pan_x: clamp(view.pan_x + dx / travel, -1, 1),
    pan_y: clamp(view.pan_y + dy / travel, -1, 1),
  };
}

/** Where the crop sits inside the camera picture, in percent. */
export function cropToPercent(crop: Rect, src: [number, number]) {
  const [w, h] = src;
  if (w <= 0 || h <= 0 || crop.w <= 0 || crop.h <= 0) return { left: 0, top: 0, width: 100, height: 100 };
  return { left: (crop.x / w) * 100, top: (crop.y / h) * 100, width: (crop.w / w) * 100, height: (crop.h / h) * 100 };
}

const NAMED_KEYS: Record<string, string> = {
  Equal: "=",
  Minus: "-",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ArrowUp: "Up",
  ArrowDown: "Down",
  Space: "Space",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Comma: ",",
  Period: ".",
  BracketLeft: "[",
  BracketRight: "]",
};

type KeyLike = Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;

/**
 * Turns a key press into a hotkey string such as "Ctrl+Alt+=". Returns null
 * while only modifier keys are down, for keys with no name here, and for a
 * plain key without Ctrl/Alt/Win (which would swallow normal typing in every
 * app). Function keys are allowed on their own.
 */
export function eventToAccelerator(e: KeyLike): string | null {
  let key: string | undefined;
  const fn = /^F([1-9]|1\d|2[0-4])$/.test(e.code);
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (fn) key = e.code;
  else key = NAMED_KEYS[e.code];
  if (!key) return null;
  if (!fn && !e.ctrlKey && !e.altKey && !e.metaKey) return null;
  const parts = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super", key].filter(Boolean);
  return parts.join("+");
}

export const ACTION_LABELS: Record<HotkeyAction, string> = {
  zoom_in: "Zoom in",
  zoom_out: "Zoom out",
  reset: "Reset",
  pan_left: "Move left",
  pan_right: "Move right",
  pan_up: "Move up",
  pan_down: "Move down",
};

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { CameraPicker } from "./components/CameraPicker";
import { HotkeyList } from "./components/HotkeyList";
import { Preview } from "./components/Preview";
import { StatusLine } from "./components/StatusLine";
import { ZoomSlider } from "./components/ZoomSlider";
import { api, type AppState, type Camera, type HotkeyAction, type Output, type Preview as PreviewData, type View } from "./lib/api";

const HOME: View = { zoom: 1, pan_x: 0, pan_y: 0 };

export default function App() {
  const [state, setState] = useState<AppState | null>(null);
  const [cameras, setCameras] = useState<Camera[]>([]);
  const [preview, setPreview] = useState<PreviewData | null>(null);
  const [view, setView] = useState<View>(HOME);
  const [startError, setStartError] = useState<string | null>(null);
  // while the user is moving the slider or frame, their hand wins over polled state
  const touchedAt = useRef(0);

  const refreshState = useCallback(async () => {
    const next = await api.getState();
    setState(next);
    if (Date.now() - touchedAt.current > 600) setView(next.status.view);
  }, []);
  const refreshCameras = useCallback(() => {
    api.listCameras().then(setCameras, () => setCameras([]));
  }, []);

  useEffect(() => {
    refreshState();
    refreshCameras();
    const stateTimer = setInterval(refreshState, 400);
    const cameraTimer = setInterval(refreshCameras, 5000);
    const unlisten = listen("view-changed", () => {
      touchedAt.current = 0;
      refreshState();
    });
    return () => {
      clearInterval(stateTimer);
      clearInterval(cameraTimer);
      unlisten.then((off) => off());
    };
  }, [refreshState, refreshCameras]);

  // Whether the camera runs for the preview is decided by the app itself from
  // the real window state (shown, hidden, minimised), not by this page.

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      if (!document.hidden) {
        try {
          setPreview(await api.getPreview());
        } catch {
          /* the next tick tries again */
        }
      }
      if (alive) timer = setTimeout(tick, 66);
    };
    tick();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, []);

  const changeView = useCallback((next: View) => {
    touchedAt.current = Date.now();
    setView(next);
    api.setView(next);
  }, []);
  const resetView = useCallback(() => changeView(HOME), [changeView]);

  const changeHotkey = useCallback(
    (action: HotkeyAction, accelerator: string) => {
      api.setHotkey(action, accelerator).then(refreshState, refreshState);
    },
    [refreshState],
  );

  if (!state) return null;
  const { settings, status } = state;

  return (
    <div className="shell">
      <main className="stage">
        <header className="flex items-baseline justify-between">
          <span className="wordmark">anyzoom</span>
          <span className="readout">
            PAN // {view.pan_x >= 0 ? "+" : "−"}
            {Math.abs(view.pan_x).toFixed(2)} {view.pan_y >= 0 ? "+" : "−"}
            {Math.abs(view.pan_y).toFixed(2)}
          </span>
        </header>
        <Preview preview={preview} status={status} view={view} onView={changeView} />
        <ZoomSlider view={view} onView={changeView} onReset={resetView} />
      </main>

      <aside className="side">
        <section className="group">
          <h3>Camera</h3>
          <CameraPicker
            cameras={cameras}
            selected={settings.camera_id}
            onRefresh={refreshCameras}
            onSelect={(id) => api.setCamera(id).then(refreshState)}
          />
          <p className="hint">In your call app, choose “anyzoom Camera”.</p>
        </section>

        <section className="group">
          <h3>Picture sent to apps</h3>
          <div className="segmented">
            {(["720p", "1080p"] as Output[]).map((o) => (
              <button key={o} type="button" aria-pressed={settings.output === o} onClick={() => api.setOutput(o).then(refreshState)}>
                {o}
              </button>
            ))}
          </div>
          <div className="mt-3 flex items-center justify-between">
            <span className="hint" style={{ margin: 0 }}>
              Scroll or drag on the picture to frame it.
            </span>
            <button type="button" className="btn" onClick={resetView}>
              Reset
            </button>
          </div>
        </section>

        <section className="group">
          <h3>Hotkeys</h3>
          <HotkeyList hotkeys={settings.hotkeys} errors={state.hotkey_errors} onChange={changeHotkey} />
          <p className="hint">They work while any app is in front. Click one to change it.</p>
        </section>

        <section className="group">
          <label className="switch">
            <span>
              Start with Windows
              <span className="hint" style={{ display: "block", margin: 0 }}>
                The camera is ready before your call starts.
              </span>
            </span>
            <input
              type="checkbox"
              checked={settings.start_with_windows}
              onChange={(e) =>
                api
                  .setStartWithWindows(e.target.checked)
                  .then(() => setStartError(null), (err) => setStartError(String(err)))
                  .then(refreshState)
              }
            />
          </label>
          {startError && <p className="error">Could not change this: {startError}</p>}
        </section>
      </aside>

      <StatusLine status={status} version={state.version} />
    </div>
  );
}

import type { Status } from "../lib/api";

/** One line of truth about the virtual camera, in the 3E readout style. */
export function StatusLine({ status, version }: { status: Status; version: string }) {
  let state: JSX.Element;
  if (status.vcam_error) {
    const notInstalled = status.vcam_error.includes("80040154");
    state = (
      <span className="bad">
        {notInstalled ? "VCAM // NOT INSTALLED — run install.cmd from the anyzoom folder" : `VCAM // OFF — ${status.vcam_error}`}
      </span>
    );
  } else if (status.camera_error) {
    state = (
      <>
        <strong>VCAM // ONLINE</strong> · <span className="bad">CAMERA UNAVAILABLE</span>
      </>
    );
  } else if (status.consumer_active) {
    state = (
      <>
        <strong>VCAM // ONLINE</strong> · <span className="live">IN USE</span>
      </>
    );
  } else {
    state = (
      <>
        <strong>VCAM // ONLINE</strong> · IDLE
      </>
    );
  }
  const source = status.camera_open && status.src_size ? `${status.src_size[0]}×${status.src_size[1]} @ ${status.fps.toFixed(0)} FPS` : "CAMERA OFF";
  return (
    <footer className="statusline">
      <span>{state}</span>
      <span>
        {source} · v{version}
      </span>
    </footer>
  );
}

import { useRef, useState } from "react";
import { cropToPercent, dragToPan, formatZoom, wheelToZoom, type Preview as PreviewData, type Status, type View } from "../lib/api";

type Props = {
  preview: PreviewData | null;
  status: Status;
  view: View;
  onView: (view: View) => void;
};

/** The whole camera picture, with a frame around the part other apps receive. */
export function Preview({ preview, status, view, onView }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; view: View } | null>(null);
  const [dragging, setDragging] = useState(false);

  const src = preview?.src ?? status.src_size ?? [16, 9];
  const aspect = src[0] > 0 && src[1] > 0 ? `${src[0]} / ${src[1]}` : "16 / 9";

  const notice = status.camera_error
    ? { title: "Camera unavailable", body: cameraHelp(status.camera_error) }
    : !preview
      ? { title: "Starting the camera", body: "The picture appears here in a moment." }
      : null;

  const crop = preview ? cropToPercent(preview.crop, preview.src) : null;

  return (
    <div className="preview-wrap">
      <div
        ref={box}
        className="preview"
        style={{ aspectRatio: aspect, height: "100%" }}
        onWheel={(e) => onView({ ...view, zoom: wheelToZoom(view.zoom, e.deltaY) })}
      >
        {preview && <img src={`data:image/jpeg;base64,${preview.jpeg_base64}`} alt="Live picture from your camera" draggable={false} />}
        {crop && !notice && (
          <div
            className={`crop${dragging ? " dragging" : ""}${view.zoom <= 1.001 ? " fixed" : ""}`}
            style={{ left: `${crop.left}%`, top: `${crop.top}%`, width: `${crop.width}%`, height: `${crop.height}%` }}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
              drag.current = { x: e.clientX, y: e.clientY, view };
              setDragging(true);
            }}
            onPointerMove={(e) => {
              const start = drag.current;
              const size = box.current?.getBoundingClientRect();
              if (!start || !size || size.width === 0) return;
              onView(dragToPan(start.view, (e.clientX - start.x) / size.width, (e.clientY - start.y) / size.height));
            }}
            onPointerUp={() => {
              drag.current = null;
              setDragging(false);
            }}
            onPointerCancel={() => {
              drag.current = null;
              setDragging(false);
            }}
            onDoubleClick={() => onView({ zoom: 1, pan_x: 0, pan_y: 0 })}
          >
            <span className={`crop-tag${crop.top < 6 ? " inside" : ""}`}>ZOOM // {formatZoom(view.zoom)}</span>
          </div>
        )}
        {notice && (
          <div className="notice">
            <h2>{notice.title}</h2>
            <p>{notice.body}</p>
          </div>
        )}
      </div>
    </div>
  );
}

function cameraHelp(error: string): string {
  if (error.includes("in use")) {
    return "Another app is using this camera, or Windows privacy settings block it. Close the other app, or point it at “anyzoom Camera” instead. anyzoom keeps trying.";
  }
  if (error.includes("not found")) {
    return "No camera is connected, or the chosen one was unplugged. Plug it in or pick another camera. anyzoom keeps trying.";
  }
  if (error.includes("disconnected")) {
    return "The camera stopped sending video. Check the cable. anyzoom keeps trying.";
  }
  return `${error}. anyzoom keeps trying.`;
}

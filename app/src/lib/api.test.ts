import { describe, expect, it } from "vitest";
import { cropToPercent, dragToPan, eventToAccelerator, formatZoom, wheelToZoom } from "./api";

describe("formatZoom", () => {
  it("shows two decimals and a multiplication sign", () => {
    expect(formatZoom(1.5)).toBe("1.50×");
    expect(formatZoom(1)).toBe("1.00×");
    expect(formatZoom(3.999)).toBe("4.00×");
  });
});

describe("wheelToZoom", () => {
  it("zooms in when the wheel rolls away from the user", () => {
    expect(wheelToZoom(2, -100)).toBeGreaterThan(2);
    expect(wheelToZoom(2, 100)).toBeLessThan(2);
  });
  it("stays within 1x and 4x", () => {
    expect(wheelToZoom(3.9, -100000)).toBe(4);
    expect(wheelToZoom(1.1, 100000)).toBe(1);
  });
  it("ignores a broken wheel event", () => {
    expect(wheelToZoom(2, Number.NaN)).toBe(2);
  });
});

describe("dragToPan", () => {
  const view = { zoom: 2, pan_x: 0, pan_y: 0 };
  it("moves the frame with the pointer", () => {
    // at 2x the frame can travel a quarter of the picture each way
    const moved = dragToPan(view, 0.125, -0.125);
    expect(moved.pan_x).toBeCloseTo(0.5);
    expect(moved.pan_y).toBeCloseTo(-0.5);
    expect(moved.zoom).toBe(2);
  });
  it("clamps to the edges", () => {
    const moved = dragToPan(view, 5, -5);
    expect(moved.pan_x).toBe(1);
    expect(moved.pan_y).toBe(-1);
  });
  it("does nothing at 1x, where there is nowhere to go", () => {
    expect(dragToPan({ zoom: 1, pan_x: 0, pan_y: 0 }, 0.3, 0.3)).toEqual({ zoom: 1, pan_x: 0, pan_y: 0 });
  });
});

describe("cropToPercent", () => {
  it("places the crop inside the picture", () => {
    expect(cropToPercent({ x: 480, y: 270, w: 960, h: 540 }, [1920, 1080])).toEqual({ left: 25, top: 25, width: 50, height: 50 });
  });
  it("survives an empty source", () => {
    expect(cropToPercent({ x: 0, y: 0, w: 0, h: 0 }, [0, 0])).toEqual({ left: 0, top: 0, width: 100, height: 100 });
  });
});

describe("eventToAccelerator", () => {
  const key = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
    code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });
  it("builds the default bindings", () => {
    expect(eventToAccelerator(key("Equal", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+=");
    expect(eventToAccelerator(key("Minus", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+-");
    expect(eventToAccelerator(key("Digit0", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+0");
    expect(eventToAccelerator(key("ArrowLeft", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+Left");
  });
  it("handles letters, shift and function keys", () => {
    expect(eventToAccelerator(key("KeyZ", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+Z");
    expect(eventToAccelerator(key("F9"))).toBe("F9");
  });
  it("waits while only modifiers are held", () => {
    expect(eventToAccelerator(key("ControlLeft", { ctrlKey: true }))).toBeNull();
  });
  it("refuses a bare key that would break typing everywhere", () => {
    expect(eventToAccelerator(key("KeyA"))).toBeNull();
    expect(eventToAccelerator(key("KeyA", { shiftKey: true }))).toBeNull();
  });
  it("refuses keys it cannot name", () => {
    expect(eventToAccelerator(key("IntlBackslash", { ctrlKey: true }))).toBeNull();
  });
});

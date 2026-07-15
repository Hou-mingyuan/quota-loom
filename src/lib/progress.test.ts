import { describe, expect, it } from "vitest";
import { floatingMeterPercent } from "./progress";

describe("floatingMeterPercent", () => {
  it("uses weekly remaining quota when it is available", () => {
    expect(floatingMeterPercent(68, 0.95)).toBe(68);
    expect(floatingMeterPercent(0, 0.95)).toBe(0);
    expect(floatingMeterPercent(120, 0.95)).toBe(100);
  });

  it("falls back to cache hit rate without weekly quota", () => {
    expect(floatingMeterPercent(null, 0.95)).toBe(95);
    expect(floatingMeterPercent(null, 0)).toBe(4);
    expect(floatingMeterPercent(null, 1.5)).toBe(100);
  });
});

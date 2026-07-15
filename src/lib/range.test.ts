import { describe, expect, it } from "vitest";
import { resolveRange } from "./range";

describe("resolveRange", () => {
  const now = new Date(2026, 6, 15, 13, 45, 30, 900);

  it("uses local midnight for today", () => {
    const range = resolveRange("today", now);
    const midnight = new Date(now);
    midnight.setHours(0, 0, 0, 0);

    expect(range.startAt).toBe(Math.floor(midnight.getTime() / 1000));
    expect(range.endAt).toBe(Math.floor(now.getTime() / 1000));
    expect(range.bucketSeconds).toBe(3_600);
    expect(range.timezoneOffsetSeconds).toBe(-now.getTimezoneOffset() * 60);
  });

  it.each([
    ["24h", 86_400, 3_600],
    ["7d", 604_800, 21_600],
    ["30d", 2_592_000, 86_400],
  ] as const)("resolves %s to a fixed duration", (preset, duration, bucket) => {
    const range = resolveRange(preset, now);

    expect(range.endAt - range.startAt).toBe(duration);
    expect(range.bucketSeconds).toBe(bucket);
  });
});

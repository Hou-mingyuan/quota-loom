import type { RangePreset, UsageRange } from "../types";

export function resolveRange(
  preset: RangePreset,
  now = new Date(),
): UsageRange {
  const endAt = Math.floor(now.getTime() / 1000);
  const timezoneOffsetSeconds = -now.getTimezoneOffset() * 60;
  if (preset === "today") {
    const start = new Date(now);
    start.setHours(0, 0, 0, 0);
    return {
      startAt: Math.floor(start.getTime() / 1000),
      endAt,
      bucketSeconds: 3600,
      timezoneOffsetSeconds,
    };
  }
  const seconds =
    preset === "24h" ? 86_400 : preset === "7d" ? 604_800 : 2_592_000;
  return {
    startAt: endAt - seconds,
    endAt,
    bucketSeconds: preset === "24h" ? 3600 : preset === "7d" ? 21_600 : 86_400,
    timezoneOffsetSeconds,
  };
}

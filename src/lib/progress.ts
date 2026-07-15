function clampPercent(value: number, minimum: number) {
  if (!Number.isFinite(value)) return minimum;
  return Math.min(100, Math.max(minimum, value));
}

export function floatingMeterPercent(
  weeklyRemainingPercent: number | null,
  cacheHitRate: number,
) {
  if (weeklyRemainingPercent !== null) {
    return clampPercent(weeklyRemainingPercent, 0);
  }
  return clampPercent(cacheHitRate * 100, 4);
}

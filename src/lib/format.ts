export function formatTokens(value: number) {
  if (value >= 1_000_000_000) return `${trim(value / 1_000_000_000)}B`;
  if (value >= 1_000_000) return `${trim(value / 1_000_000)}M`;
  if (value >= 1_000) return `${trim(value / 1_000)}K`;
  return value.toLocaleString("zh-CN");
}

export function formatInteger(value: number) {
  return value.toLocaleString("zh-CN");
}

export function formatCost(value: string | null, fractionDigits?: number) {
  if (value === null) return "未定价";
  const parsed = Number(value);
  const digits = fractionDigits ?? (parsed >= 10 ? 2 : 4);
  return Number.isFinite(parsed) ? `$${parsed.toFixed(digits)}` : "未定价";
}

export function formatPercent(value: number) {
  return `${Math.round(value * 100)}%`;
}

export function formatTime(timestamp: number, includeDate = false) {
  return new Date(timestamp * 1000).toLocaleString("zh-CN", {
    month: includeDate ? "2-digit" : undefined,
    day: includeDate ? "2-digit" : undefined,
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export function formatResetTime(timestamp: number) {
  return new Date(timestamp * 1000).toLocaleString("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function trim(value: number) {
  return value
    .toFixed(value >= 100 ? 1 : value >= 10 ? 2 : 3)
    .replace(/\.?0+$/, "");
}

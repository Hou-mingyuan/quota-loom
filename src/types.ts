export interface UsageRange {
  startAt: number;
  endAt: number;
  bucketSeconds: number;
  timezoneOffsetSeconds: number;
}

export interface UsageSummary {
  totalTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  calls: number;
  threads: number;
  cacheHitRate: number;
  estimatedCostUsd: string | null;
}

export interface UsageTrendPoint {
  bucketStart: number;
  totalTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  calls: number;
}

export interface DailyCostPoint {
  dayStart: number;
  estimatedCostUsd: string | null;
}

export interface ModelUsage {
  model: string;
  totalTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  calls: number;
  cacheHitRate: number;
  estimatedCostUsd: string | null;
}

export interface ModelPriceEntry {
  model: string;
  inputPerMillion: string;
  cachedInputPerMillion: string;
  outputPerMillion: string;
  multiplier: string;
  configured: boolean;
}

export interface RecentUsageEvent {
  id: string;
  threadId: string;
  occurredAt: number;
  model: string;
  totalTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  estimatedCostUsd: string | null;
}

export interface UsageSnapshot {
  generatedAt: number;
  codexHome: string;
  sourceKind: "claudeCode" | "codexCli" | "chatGptCodex";
  sourceLabel: string;
  sourceBrand: string;
  summary: UsageSummary;
  trends: UsageTrendPoint[];
  dailyCosts: DailyCostPoint[];
  models: ModelUsage[];
  recent: RecentUsageEvent[];
}

export interface WeeklyUsage {
  usedPercent: number;
  remainingPercent: number;
  resetsAt: number | null;
}

export interface SyncResult {
  scannedFiles: number;
  changedFiles: number;
  importedEvents: number;
  rebuiltFiles: number;
  warnings: string[];
}

export type RangePreset = "today" | "24h" | "7d" | "30d";

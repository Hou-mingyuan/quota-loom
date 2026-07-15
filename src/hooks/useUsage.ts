import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  getAccountWeeklyUsage,
  getUsageSnapshot,
  isDesktopRuntime,
} from "../lib/api";
import { resolveRange } from "../lib/range";
import type { RangePreset } from "../types";

export function useUsage(preset: RangePreset) {
  return useQuery({
    queryKey: ["usage", preset],
    queryFn: () => getUsageSnapshot(resolveRange(preset)),
    placeholderData: keepPreviousData,
    refetchInterval: 15_000,
    refetchIntervalInBackground: true,
  });
}

export function useWeeklyUsage() {
  return useQuery({
    queryKey: ["account-weekly-usage"],
    queryFn: getAccountWeeklyUsage,
    refetchInterval: 60_000,
    refetchIntervalInBackground: true,
    retry: false,
    staleTime: 45_000,
  });
}

export function useUsageEvents() {
  const client = useQueryClient();
  useEffect(() => {
    if (!isDesktopRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("usage-updated", () => {
      void client.invalidateQueries({ queryKey: ["usage"] });
    }).then((off) => {
      if (disposed) off();
      else unlisten = off;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [client]);
}

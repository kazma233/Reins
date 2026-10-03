import { invoke } from "@tauri-apps/api/core";
import type { UsageStats } from "./types";

export function getUsageStats(): Promise<UsageStats> {
  return invoke("get_usage_stats");
}

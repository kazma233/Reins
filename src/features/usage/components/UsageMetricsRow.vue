<script setup lang="ts">
import { formatTokenCount } from "@shared/lib/format";
import { formatCacheHitRate, type UsageMetrics } from "../model";
import "./usage-metrics-row.css";

type UsageMetricsRowProps = {
  metrics: UsageMetrics;
  // 今日窗口没有"日均"概念,峰值从"日"换成"小时"。
  mode?: "window" | "today";
};

withDefaults(defineProps<UsageMetricsRowProps>(), { mode: "window" });
</script>

<template>
  <div class="usage-metrics">
    <div class="usage-metric usage-metric--primary">
      <span class="usage-metric__label">总消耗</span>
      <span class="usage-metric__value">{{ formatTokenCount(metrics.totalTokens) }}</span>
      <span class="usage-metric__meta">
        输入 {{ formatTokenCount(metrics.inputTokens) }} · 输出
        {{ formatTokenCount(metrics.outputTokens) }} · 缓存读
        {{ formatTokenCount(metrics.cacheReadTokens) }} · 缓存写
        {{ formatTokenCount(metrics.cacheWriteTokens) }}
      </span>
    </div>
    <div v-if="mode === 'window'" class="usage-metric">
      <span class="usage-metric__label">日均消耗</span>
      <span class="usage-metric__value">{{ formatTokenCount(metrics.dailyAverage) }}</span>
      <span class="usage-metric__meta">按窗口内自然日平均</span>
    </div>
    <div class="usage-metric">
      <span class="usage-metric__label">{{ mode === "today" ? "峰值小时" : "峰值日" }}</span>
      <span class="usage-metric__value">
        {{ metrics.peak ? formatTokenCount(metrics.peak.tokens) : "—" }}
      </span>
      <span class="usage-metric__meta">
        {{
          metrics.peak
            ? mode === "today"
              ? `${metrics.peak.bucket}:00`
              : metrics.peak.bucket
            : mode === "today"
              ? "今日无消耗"
              : "窗口内无消耗"
        }}
      </span>
    </div>
    <div class="usage-metric">
      <span class="usage-metric__label">缓存命中率</span>
      <span class="usage-metric__value">{{ formatCacheHitRate(metrics.cacheHitRate) }}</span>
      <span class="usage-metric__meta">缓存读 / (输入 + 缓存读)</span>
    </div>
    <div class="usage-metric">
      <span class="usage-metric__label">{{ mode === "today" ? "今日有用量会话" : "有用量会话" }}</span>
      <span class="usage-metric__value">{{ metrics.sessionCount.toLocaleString("zh-CN") }}</span>
      <span class="usage-metric__meta">{{
        mode === "today" ? "今日产生消耗的会话数" : "全周期累计,不随窗口变化"
      }}</span>
    </div>
  </div>
</template>

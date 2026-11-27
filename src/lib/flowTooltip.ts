import { format } from "echarts/core";

/** HTML tooltip body. Every interpolated value is escaped before it reaches ECharts innerHTML. */
export function flowTooltipHtml(kind: "edge" | "node", from: string, to: string, label: string, valueLine: string): string {
  const esc = (value: string) => format.encodeHTML(value);
  if (kind === "edge") return `${esc(from)} → ${esc(to)}<br/>${esc(valueLine)}`;
  return `${esc(label)}<br/>${esc(valueLine)}`;
}

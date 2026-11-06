/** https links the UI may hand to the system opener. */
export function safeHttps(value: string): boolean {
  try {
    const url = new URL(value);
    if (url.protocol !== "https:") return false;
    if (url.username || url.password) return false;
    const host = url.hostname.replace(/\.$/, "").toLowerCase();
    if (!host || host === "localhost" || host.endsWith(".localhost") || host.endsWith(".local")) return false;
    if (host.includes(":") || /^\d{1,3}(\.\d{1,3}){3}$/.test(host)) return false;
    return host.includes(".");
  } catch {
    return false;
  }
}

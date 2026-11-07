const UPDATE_VERSION = /^v?\d{1,6}(?:\.\d{1,6}){0,3}$/;

/**
 * Update links the banner may open. Same hosts as `update_url_allowed` in Rust:
 * this repo's GitHub release APK or release page, or the known landing page.
 */
export function safeUpdateUrl(value: string): boolean {
  if (value.length > 200 || /[\s\\@]/.test(value)) return false;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash || url.port) return false;
    const host = url.hostname.replace(/\.$/, "").toLowerCase();
    const parts = url.pathname.split("/").filter(Boolean);
    if (parts.some((part) => part.includes(".."))) return false;
    if (host === "github.com") {
      if (parts[0] !== "Gabo-Tech" || parts[1] !== "grok-voice-android" || parts[2] !== "releases") return false;
      if (parts.length === 5 && parts[3] === "tag" && UPDATE_VERSION.test(parts[4])) return true;
      if (parts.length === 6 && parts[3] === "download" && UPDATE_VERSION.test(parts[4])) {
        const bare = parts[4].replace(/^v/, "");
        return parts[5] === `GrokVoice-${bare}.apk`;
      }
      return false;
    }
    return host === "gabo-tech.github.io" && parts.length === 1 && parts[0] === "grok-voice-android";
  } catch {
    return false;
  }
}

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

import { useEffect, useState } from "react";

type MotionBridge = { animationsEnabled?: () => boolean };

function androidAllowsMotion() {
  const bridge = (window as unknown as { GrokMotion?: MotionBridge }).GrokMotion;
  if (bridge?.animationsEnabled) {
    try {
      return bridge.animationsEnabled();
    } catch {
      return true;
    }
  }
  return document.documentElement.dataset.animator !== "off";
}

/** False when the person asked for less motion, or Android's animator scale is off. */
export function useMotionAllowed() {
  const [allowed, setAllowed] = useState(true);
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const read = () => setAllowed(!media.matches && androidAllowsMotion());
    read();
    media.addEventListener("change", read);
    const obs = new MutationObserver(read);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-animator"] });
    return () => {
      media.removeEventListener("change", read);
      obs.disconnect();
    };
  }, []);
  return allowed;
}

/** Light themes paint the character with a dark outline so the white face stays visible. */
export function useLightScheme() {
  const [light, setLight] = useState(false);
  useEffect(() => {
    const read = () => {
      const scheme = getComputedStyle(document.documentElement).getPropertyValue("--scheme").trim();
      setLight(scheme === "light");
    };
    read();
    const obs = new MutationObserver(read);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-appearance", "style", "class"] });
    return () => obs.disconnect();
  }, []);
  return light;
}

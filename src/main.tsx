import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ShotPage } from "./shot/ShotPage";
import { bootAppearance, normalizeAppearance, rememberAppearance } from "@/lib/appearance";
import { installHistory } from "@/lib/history";
import { installKeyboardInset } from "@/lib/keyboard";
import "./index.css";
const askedAppearance = new URLSearchParams(window.location.search).get("appearance");
if (askedAppearance) rememberAppearance(normalizeAppearance(askedAppearance));
bootAppearance();
installKeyboardInset();
installHistory();

const previewZoom = Number(new URLSearchParams(window.location.search).get("textZoom") || "0");
if (previewZoom > 100 && previewZoom <= 300) {
  const factor = previewZoom / 100;
  window.setTimeout(() => {
    const nodes = [document.documentElement, document.body, ...document.body.querySelectorAll<HTMLElement>("*")];
    const measured = nodes.map((el) => {
      const style = getComputedStyle(el);
      return { el, size: Number.parseFloat(style.fontSize), line: style.lineHeight };
    });
    for (const { el, size, line } of measured) {
      if (!Number.isFinite(size) || size <= 0) continue;
      el.style.fontSize = `${(size * factor).toFixed(2)}px`;
      if (line.endsWith("px")) el.style.lineHeight = `${(Number.parseFloat(line) * factor).toFixed(2)}px`;
    }
    const measureBar = () => {
      const bar = document.querySelector(".bottom-nav");
      if (bar) document.documentElement.style.setProperty("--nav-block", `${Math.ceil(bar.getBoundingClientRect().height)}px`);
    };
    window.setTimeout(measureBar, 50);
    window.setTimeout(measureBar, 500);
  }, 80);
}

const shotParams = new URLSearchParams(window.location.search);
const shot = shotParams.get("shot");
if (shot?.startsWith("flow")) {
  const group = shotParams.get("flow") === "entry" ? "entry" : "category";
  const labels = shotParams.get("labels") !== "0";
  const portraitLayout = shotParams.get("portraitLayout") === "horizontal" ? "horizontal" : "vertical";
  localStorage.setItem("grok-flow-options", JSON.stringify({ labels, values: true, group, mono: false, planned: false, portraitLayout }));
  localStorage.removeItem("grok-flow-colors");
}
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {shot ? <ShotPage kind={shot} /> : <App />}
  </React.StrictMode>,
);

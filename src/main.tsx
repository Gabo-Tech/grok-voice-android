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

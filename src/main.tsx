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

const shot = new URLSearchParams(window.location.search).get("shot");
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {shot ? <ShotPage kind={shot} /> : <App />}
  </React.StrictMode>,
);

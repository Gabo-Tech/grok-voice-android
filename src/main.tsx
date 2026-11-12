import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { bootAppearance } from "@/lib/appearance";
import { installHistory } from "@/lib/history";
import { installKeyboardInset } from "@/lib/keyboard";
import "./index.css";

bootAppearance();
installKeyboardInset();
installHistory();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

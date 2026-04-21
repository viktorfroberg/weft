import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
// Terminal font @font-face registrations — must come before xterm mounts.
// Not all four fonts are loaded at runtime; the CSS just registers their
// URLs so the browser fetches the one the user selects.
import "@fontsource-variable/jetbrains-mono";
import "@fontsource-variable/fira-code";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/source-code-pro";
import { applyInitialTheme } from "./lib/theme";

// Apply saved theme BEFORE React mounts so there's no light-theme flash
// on cold load of a dark-pref user.
applyInitialTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

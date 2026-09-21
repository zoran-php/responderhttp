// http_client/src/main.tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

removePreloader();

/**
 * The #preloader markup and its styles live in index.html, inlined, so they
 * can paint before this module (and the Tailwind bundle it pulls in) has
 * even loaded — see the comment there. React's initial commit for a root
 * with no Suspense boundary is synchronous, so by the time render() above
 * returns, #root already has content queued for the same paint that this
 * fade-out targets; there is no gap for a blank frame in between.
 */
function removePreloader(): void {
  const preloader = document.getElementById("preloader");
  if (!preloader) {
    return;
  }
  preloader.classList.add("preloader-hidden");
  preloader.addEventListener("transitionend", () => preloader.remove(), { once: true });
}

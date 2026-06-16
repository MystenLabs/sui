// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tracks Kapa sidebar open/close state and toggles a CSS class on <html>.
// Kapa renders in Shadow DOM so CSS :has() can't detect it.
// We hook window.Kapa.open/close and re-hook on every interval tick
// in case Kapa re-initializes (e.g. after script reload).

if (typeof window !== "undefined") {
  const OPEN_CLASS = "kapa-sidebar-open";
  let kapaOpen = false;
  let lastOpenRef = null;
  let lastCloseRef = null;

  function syncClass() {
    document.documentElement.classList.toggle(OPEN_CLASS, kapaOpen);
  }

  function hookKapa() {
    if (!window.Kapa || !window.Kapa.open) return;

    // Only re-hook if Kapa.open has changed (was re-initialized)
    if (window.Kapa.open === lastOpenRef) return;

    const origOpen = window.Kapa.open;
    const origClose = window.Kapa.close;

    window.Kapa.open = function (...args) {
      kapaOpen = true;
      syncClass();
      return origOpen.apply(this, args);
    };

    window.Kapa.close = function (...args) {
      kapaOpen = false;
      syncClass();
      return origClose.apply(this, args);
    };

    lastOpenRef = window.Kapa.open;
    lastCloseRef = window.Kapa.close;
  }

  // Re-sync class on every SPA navigation so new pages adjust immediately
  // Docusaurus uses History API for client-side routing
  const origPushState = history.pushState.bind(history);
  const origReplaceState = history.replaceState.bind(history);
  history.pushState = function (...args) {
    const result = origPushState(...args);
    syncClass();
    return result;
  };
  history.replaceState = function (...args) {
    const result = origReplaceState(...args);
    syncClass();
    return result;
  };
  window.addEventListener("popstate", syncClass);

  // Check every 500ms: re-hook if needed, and also detect close via
  // Shadow DOM heuristic (container height changes when sidebar closes)
  setInterval(() => {
    hookKapa();
    // Always re-sync class in case something cleared it
    syncClass();

    // Fallback: if kapa is supposedly open but the container has no
    // visible shadow content, it was closed via overlay/ESC/X
    if (kapaOpen) {
      const container = document.getElementById("kapa-widget-container");
      if (container) {
        const shadow = container.shadowRoot;
        if (shadow) {
          // Check if the shadow DOM has a visible dialog
          const dialog = shadow.querySelector("[role='dialog']") ||
                         shadow.querySelector("[class*='Modal']") ||
                         shadow.querySelector("[class*='modal']");
          if (!dialog) {
            kapaOpen = false;
            syncClass();
          }
        } else {
          // No shadow root accessible — check container dimensions
          // Kapa sidebar is ~500px wide when open, 0 when closed
          const rect = container.getBoundingClientRect();
          if (rect.width < 10 && rect.height < 10) {
            kapaOpen = false;
            syncClass();
          }
        }
      }
    }
  }, 500);
}

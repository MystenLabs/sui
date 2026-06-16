// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tracks Kapa sidebar open/close state and toggles a CSS class on <html>.
// Kapa renders in Shadow DOM so we can't query its internal DOM.
// Strategy: hook Kapa.open to detect open, then poll for close by checking
// if the Kapa container's Shadow DOM has any visible content.

if (typeof window !== "undefined") {
  const OPEN_CLASS = "kapa-sidebar-open";
  let kapaOpen = false;
  let hookedOpenRef = null;

  function syncClass() {
    document.documentElement.classList.toggle(OPEN_CLASS, kapaOpen);
  }

  function hookKapa() {
    if (!window.Kapa || !window.Kapa.open) return;
    if (window.Kapa.open === hookedOpenRef) return;

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

    hookedOpenRef = window.Kapa.open;
  }

  // Detect close: Kapa's sidebar creates a fixed-position element that's
  // visible on screen when open. Check if any direct child of body (after
  // #kapa-widget-container) has substantial dimensions — that's the sidebar.
  function isKapaSidebarVisible() {
    const container = document.getElementById("kapa-widget-container");
    if (!container) return false;

    // Check all siblings after the container for Kapa's portal
    let sibling = container.nextElementSibling;
    while (sibling) {
      // Skip known non-Kapa elements (recaptcha, textarea, etc.)
      if (sibling.id === "__docusaurus") { sibling = sibling.nextElementSibling; continue; }

      // Look for a fixed/absolute positioned element with substantial size
      const style = window.getComputedStyle(sibling);
      const rect = sibling.getBoundingClientRect();
      if (
        (style.position === "fixed" || style.position === "absolute") &&
        rect.width > 100 &&
        rect.height > 100
      ) {
        return true;
      }
      sibling = sibling.nextElementSibling;
    }

    // Also check the container itself (closed shadow DOM won't expose children
    // but the container might get dimensions when open)
    const containerRect = container.getBoundingClientRect();
    if (containerRect.width > 100 && containerRect.height > 100) {
      return true;
    }

    return false;
  }

  // Navigation hooks
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

  // Poll every 300ms
  setInterval(() => {
    hookKapa();

    if (kapaOpen) {
      // Verify it's actually still visible
      if (!isKapaSidebarVisible()) {
        kapaOpen = false;
      }
    }
    syncClass();
  }, 300);
}

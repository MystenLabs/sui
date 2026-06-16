// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Detects Kapa sidebar open/close and toggles .kapa-sidebar-open on <html>.
// Kapa renders in Shadow DOM so we can't query its internals.
// Strategy: hook Kapa.open for instant open detection, then use
// elementFromPoint to detect close (checks if right edge of screen
// is covered by a non-docusaurus element).

if (typeof window !== "undefined") {
  const OPEN_CLASS = "kapa-sidebar-open";
  let kapaOpen = false;
  let hookedRef = null;

  function syncClass() {
    document.documentElement.classList.toggle(OPEN_CLASS, kapaOpen);
  }

  function hookKapa() {
    if (!window.Kapa || !window.Kapa.open || window.Kapa.open === hookedRef) return;

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

    hookedRef = window.Kapa.open;
  }

  // Check if Kapa sidebar is covering the right side of the viewport
  function isSidebarVisible() {
    const x = window.innerWidth - 50;
    const y = window.innerHeight / 2;
    const el = document.elementFromPoint(x, y);
    if (!el) return false;
    // If the element at the right edge is inside #__docusaurus, sidebar is closed
    const docRoot = document.getElementById("__docusaurus");
    if (docRoot && docRoot.contains(el)) return false;
    // If it's the body or html itself, sidebar is closed
    if (el === document.body || el === document.documentElement) return false;
    // Otherwise something (Kapa) is covering that spot
    return true;
  }

  // Navigation hooks for SPA
  const origPush = history.pushState.bind(history);
  const origReplace = history.replaceState.bind(history);
  history.pushState = function (...args) {
    const r = origPush(...args);
    syncClass();
    return r;
  };
  history.replaceState = function (...args) {
    const r = origReplace(...args);
    syncClass();
    return r;
  };
  window.addEventListener("popstate", syncClass);

  // Poll every 300ms
  setInterval(() => {
    hookKapa();

    const visible = isSidebarVisible();
    if (visible && !kapaOpen) {
      kapaOpen = true;
    } else if (!visible && kapaOpen) {
      kapaOpen = false;
    }
    syncClass();
  }, 300);
}

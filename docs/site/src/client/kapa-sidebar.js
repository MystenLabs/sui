// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Toggles .kapa-sidebar-open on <html> when the Kapa sidebar is visible.
// Uses a persistent interval check so the class survives client-side
// navigation (Docusaurus SPA routing).

if (typeof window !== "undefined") {
  const OPEN_CLASS = "kapa-sidebar-open";
  let hooked = false;

  function hookKapaApi() {
    if (hooked || !window.Kapa) return;
    hooked = true;

    const origOpen = window.Kapa.open.bind(window.Kapa);
    const origClose = window.Kapa.close.bind(window.Kapa);

    window.Kapa.open = function (...args) {
      document.documentElement.classList.add(OPEN_CLASS);
      return origOpen(...args);
    };

    window.Kapa.close = function (...args) {
      document.documentElement.classList.remove(OPEN_CLASS);
      return origClose(...args);
    };
  }

  // Persistent check: detect Kapa sidebar open/close state by looking
  // for the widget's modal in the DOM. Runs every 300ms so it survives
  // page navigations and catches close via overlay/ESC/X button.
  setInterval(() => {
    // Try to hook the API if not done yet
    if (!hooked) hookKapaApi();

    const container = document.querySelector(".kapa-widget-container");
    // Kapa's sidebar mode renders a ModalRoot inside the container when open
    const isOpen = !!(container && container.querySelector("[role='dialog']"));
    const hasClass = document.documentElement.classList.contains(OPEN_CLASS);

    if (isOpen && !hasClass) {
      document.documentElement.classList.add(OPEN_CLASS);
    } else if (!isOpen && hasClass) {
      document.documentElement.classList.remove(OPEN_CLASS);
    }
  }, 300);
}

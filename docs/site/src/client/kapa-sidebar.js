// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Toggles a CSS class on <html> when the Kapa sidebar opens/closes
// so the main content can shrink to avoid being hidden behind it.

if (typeof window !== "undefined") {
  const OPEN_CLASS = "kapa-sidebar-open";

  function watchKapa() {
    // Kapa may not be loaded yet — poll until it is
    if (!window.Kapa) {
      setTimeout(watchKapa, 500);
      return;
    }

    // Wrap Kapa.open and Kapa.close to toggle the class
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

    // Also watch for clicks on the overlay/close button via MutationObserver
    // as a fallback in case Kapa.close isn't called directly
    const observer = new MutationObserver(() => {
      const container = document.querySelector(".kapa-widget-container");
      if (!container) return;
      const hasModal = container.innerHTML.length > 100;
      if (!hasModal) {
        document.documentElement.classList.remove(OPEN_CLASS);
      }
    });

    // Start observing once body is ready
    if (document.body) {
      observer.observe(document.body, { childList: true, subtree: true });
    } else {
      document.addEventListener("DOMContentLoaded", () => {
        observer.observe(document.body, { childList: true, subtree: true });
      });
    }
  }

  // Kick off after a short delay to let Kapa script load
  setTimeout(watchKapa, 1000);
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Watches for the Kapa AI sidebar widget to open/close and toggles
// a CSS class on <html> so the main content can adjust its width.

if (typeof window !== "undefined") {
  const OPEN_CLASS = "kapa-sidebar-open";

  const observer = new MutationObserver(() => {
    // Kapa renders its modal inside .kapa-widget-container.
    // When the sidebar is open, the container has visible children.
    const container = document.querySelector(".kapa-widget-container");
    const isOpen = container && container.querySelector("[class*='ModalContent']");
    document.documentElement.classList.toggle(OPEN_CLASS, !!isOpen);
  });

  // Observe the entire body for Kapa's dynamically injected elements
  observer.observe(document.body, { childList: true, subtree: true });
}

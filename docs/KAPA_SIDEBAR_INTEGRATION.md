# Prompt: Integrate Kapa.ai as a side panel with responsive doc content

Configure the Kapa.ai widget to open as a right-side panel (not a centered modal) so users can read documentation alongside the AI output. When the panel opens, the doc content, navbar, and TOC automatically shrink to avoid being hidden behind it. The panel persists across SPA page navigation.

---

## Overview

Three pieces work together:

1. **Kapa widget config** — `data-view-mode="sidebar"` makes Kapa render as a 500px right-side panel instead of a centered modal
2. **Client module** — JavaScript that detects when the sidebar is open/closed and toggles a CSS class on `<html>`
3. **CSS rules** — shrink body, navbar, and hide TOC when the sidebar is open

Kapa renders in Shadow DOM, so standard CSS selectors and DOM queries cannot detect its open/close state. The client module hooks `window.Kapa.open`/`close` for instant detection, with `elementFromPoint` polling as a fallback for close detection (overlay click, ESC, X button).

---

## 1. Configure the Kapa widget script

In your Docusaurus config (`docusaurus.config.js`), add these `data-*` attributes to the Kapa widget script tag:

```js
{
  src: "https://widget.kapa.ai/kapa-widget.bundle.js",
  "data-website-id": "<your-website-id>",
  "data-project-name": "<your-project-name>",
  "data-project-color": "#298DFF",
  "data-button-hide": "true",              // Hide default floating button
  "data-view-mode": "sidebar",             // Render as right-side panel
  "data-modal-title": "Ask AI",
  "data-modal-overlay-hidden": "true",     // No overlay blocking docs
  "data-modal-lock-scroll": "false",       // Allow scrolling docs while open
  "data-modal-image": "/img/logo.svg",
  async: true,
}
```

The key attributes are:
- **`data-view-mode: "sidebar"`** — switches from centered modal to right-side panel (500px default width, slides in from right)
- **`data-modal-overlay-hidden: "true"`** — no dark overlay behind the panel
- **`data-modal-lock-scroll: "false"`** — docs remain scrollable
- **`data-button-hide: "true"`** — hides Kapa's default floating button (you trigger it programmatically)

See [Kapa theming docs](https://docs.kapa.ai/integrations/website-widget/configuration/theming) for all available attributes.

---

## 2. Create the client module for open/close detection

**File:** `site/src/client/kapa-sidebar.js`

```js
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

  // Check if Kapa sidebar is covering the right side of the viewport.
  // Since Kapa uses Shadow DOM, we can't query its elements directly.
  // Instead, check if the element at the right edge of the screen
  // belongs to the doc app or to something else (Kapa's panel).
  function isSidebarVisible() {
    const x = window.innerWidth - 50;
    const y = window.innerHeight / 2;
    const el = document.elementFromPoint(x, y);
    if (!el) return false;
    const docRoot = document.getElementById("__docusaurus");
    if (docRoot && docRoot.contains(el)) return false;
    if (el === document.body || el === document.documentElement) return false;
    return true;
  }

  // Hook into History API so the class persists across SPA navigation.
  // Docusaurus uses pushState for client-side routing.
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

  // Poll every 300ms: re-hook Kapa if it reinitializes, and
  // detect open/close state via elementFromPoint fallback.
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
```

Register it in `docusaurus.config.js`:

```js
clientModules: [
  // ... existing modules
  require.resolve("./src/client/kapa-sidebar.js"),
],
```

### How the detection works

1. **Open detection:** Wraps `window.Kapa.open()` to set `kapaOpen = true` immediately when called.
2. **Close detection:** Kapa may not call its own `.close()` method when the user clicks the X button or presses ESC (it handles this internally in Shadow DOM). The fallback uses `document.elementFromPoint(rightEdge, midHeight)` every 300ms. If the pixel at the right edge of the viewport is inside `#__docusaurus`, the sidebar is closed. If it's covered by something else (Kapa's panel), it's open.
3. **SPA navigation:** Hooks `history.pushState`/`replaceState` and `popstate` to re-sync the class on every Docusaurus client-side navigation.
4. **Re-hooking:** Checks every 300ms if `window.Kapa.open` has changed (Kapa reinitializing), and re-wraps if needed.

---

## 3. Add the CSS rules

Add to your `custom.css`:

```css
/* ---- Kapa sidebar content shift ---- */
/* Kapa renders in Shadow DOM so CSS :has() can't detect it.
   A client module (kapa-sidebar.js) toggles .kapa-sidebar-open on <html>. */

/* Shrink body to make room for the 500px sidebar */
html.kapa-sidebar-open body {
  width: calc(100vw - 500px) !important;
  overflow-x: hidden;
  transition: width 0.3s ease;
}

/* Shrink the fixed navbar to match (it uses position:fixed so body width
   doesn't affect it — set right offset instead) */
html.kapa-sidebar-open .navbar,
html.kapa-sidebar-open .navbar--fixed-top {
  right: 500px !important;
  overflow: hidden;
  transition: right 0.3s ease;
}

/* Hide the Ask AI and Search buttons when the sidebar is already open */
html.kapa-sidebar-open .kapa-trigger-btn,
html.kapa-sidebar-open .DocSearch-Button {
  display: none !important;
}

/* Smooth transition when closing */
.navbar,
.navbar--fixed-top {
  transition: right 0.3s ease;
}

body {
  transition: width 0.3s ease;
}

/* Hide the desktop TOC column to free space */
html.kapa-sidebar-open .col.col--3:has(.theme-doc-toc-desktop) {
  display: none;
}

/* Let the main content column expand to fill the freed space */
html.kapa-sidebar-open .col[class*="docItemCol"] {
  max-width: 100% !important;
  flex: 1 !important;
}

/* On mobile, Kapa goes full-screen so don't shrink */
@media (max-width: 768px) {
  html.kapa-sidebar-open body {
    width: 100vw !important;
  }
}
```

### What each rule does

| Rule | Purpose |
|------|---------|
| `body { width: calc(100vw - 500px) }` | Shrinks all content to fit beside the 500px sidebar |
| `.navbar { right: 500px }` | The navbar is `position: fixed` so it ignores body width. Setting `right` shrinks it from the right edge. |
| `.navbar { overflow: hidden }` | Prevents navbar items from overflowing when narrowed |
| `.kapa-trigger-btn, .DocSearch-Button { display: none }` | Hides redundant buttons since the AI is already visible |
| `.col.col--3 { display: none }` | Hides the desktop table of contents to free horizontal space |
| `.col[docItemCol] { flex: 1 }` | Lets the main content column expand into the freed TOC space |

---

## 4. Trigger Kapa programmatically

Since `data-button-hide: "true"` hides the default floating button, trigger it from your own UI:

```js
// Open the sidebar
if (window.Kapa) window.Kapa.open();

// Open with a pre-filled query
if (window.Kapa) window.Kapa.open("How do I deploy to Sui?");

// Close
if (window.Kapa) window.Kapa.close();
```

Common trigger patterns:
- **Navbar button:** A custom button component that calls `window.Kapa.open()` on click
- **Search modal:** An "Ask AI" button inside a search modal that closes the modal then opens Kapa with the query: `onClose(); setTimeout(() => window.Kapa.open(query), 100);`
- **Landing page search bar:** A styled button that dispatches a custom event to open a combined search/AI modal

---

## Verification

1. Start the dev server
2. Click the "Ask AI" button — sidebar should slide in from the right
3. Doc content, navbar, and TOC should shrink/hide with a smooth 300ms transition
4. Navigate to another page while the sidebar is open — content should stay shrunk
5. Close the sidebar (X button, ESC) — everything should return to full width within 300ms
6. Test on mobile — sidebar should go full-screen, no content shift
7. Verify no horizontal scrollbar appears when sidebar is open

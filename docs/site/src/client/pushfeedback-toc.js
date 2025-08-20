// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const PROJECT_ID = "yap67az1qz"; // <-- your PushFeedback project id

function ensurePushFeedbackScript() {
  if (document.getElementById("__pushfeedback-cdn")) return;
  const s = document.createElement("script");
  s.id = "__pushfeedback-cdn";
  s.type = "module";
  s.src =
    "https://cdn.jsdelivr.net/npm/pushfeedback@latest/dist/pushfeedback/pushfeedback.esm.js";
  document.head.appendChild(s);
}

function styleVars() {
  // Use your Docusaurus tokens so it adapts to light/dark
  return [
    '--pf-overlay-bg: rgba(0,0,0,0.6)',
    '--pf-modal-bg: var(--ifm-background-color)',
    '--pf-text-color: var(--ifm-font-color-base)',
    '--pf-border-color: var(--ifm-color-emphasis-200)',
    '--pf-radius: 12px',
    '--pf-shadow: 0 8px 30px rgba(0,0,0,.2)',
  ].join('; ');
}

function buildLI() {
  const li = document.createElement("li");
  li.id = "__pf_toc_item";
  li.className = "table-of-contents__item toc-feedback";
  li.innerHTML = `
    <div class="toc-feedback__wrap">
      <div class="toc-feedback__title">Was this page helpful?</div>
      <div class="toc-feedback__row">
        <feedback-button
          style="${styleVars()}"
          project="${PROJECT_ID}"
          rating="1"
          custom-font="True"
          button-style="default"
          modal-position="center"
        >
          <button class="button button--outline button--primary button--sm" title="Yes">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
              stroke-linecap="round" stroke-linejoin="round">
              <path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3"></path>
            </svg>
            <span>Yes</span>
          </button>
        </feedback-button>

        <feedback-button
        style="${styleVars()}"
          project="${PROJECT_ID}"
          rating="0"
          custom-font="True"
          button-style="default"
          modal-position="center"
        >
          <button class="button button--outline button--primary button--sm" title="No">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
              stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 15v4a3 3 0 0 0 3 3l4-9V2H5.72a2 2 0 0 0-2 1.7l-1.38 9a2 2 0 0 0 2 2.3zm7-13h2.67A2.31 2.31 0 0 1 22 4v7a2.31 2.31 0 0 1-2.33 2H17"></path>
            </svg>
            <span>No</span>
          </button>
        </feedback-button>
      </div>
    </div>
  `;
  return li;
}

function injectIntoTOC() {
  // Prefer desktop TOC; fall back to any TOC ul if needed
  let ul =
    document.querySelector(".theme-doc-toc-desktop ul.table-of-contents") ||
    document.querySelector("ul.table-of-contents");

  if (!ul) return;               // no TOC on this page
  if (document.getElementById("__pf_toc_item")) return; // already inserted

  // Insert as a <li> to keep UL valid
  ul.appendChild(buildLI());
}

export function onRouteDidUpdate() {
  ensurePushFeedbackScript();

  // Try a few times to survive async TOC hydration/re-render
  const tries = [0, 100, 300, 700];
  tries.forEach((t) => setTimeout(injectIntoTOC, t));

  // As a safety net, observe TOC changes for a short period
  const observer = new MutationObserver(() => injectIntoTOC());
  observer.observe(document.body, { childList: true, subtree: true });
  setTimeout(() => observer.disconnect(), 2000);
}

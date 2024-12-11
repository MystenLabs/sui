// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// It is going to be exposed in HTTP requests anyway, so it's fine to just hardcode it here.
const COOKBOOK_PUBLIC_API_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI2NjU5ODBiNDAwZTliZDQ2MzcwZDlhNzYiLCJpYXQiOjE3MTcxNDE2ODQsImV4cCI6MjAzMjcxNzY4NH0.0JCgi4bJ_f6ILFgEyYAP-KeCm1dzOKwH30tC3jEs2_A";

async function askCookbookPlugin() {
  return {
    name: "askCookbook",
    injectHtmlTags() {
      return {
        postBodyTags: [
          {
            tagName: "div",
            attributes: {
              id: "__cookbook",
              "data-api-key": COOKBOOK_PUBLIC_API_KEY,
            },
          },
          `
            <script>
              function initCookbook() {
                try {
                  if (window.__cookbook_script) {
                    window.__cookbook_script.remove();
                  }
                  const script = document.createElement('script');
                  script.src = 'https://cdn.jsdelivr.net/npm/@cookbookdev/docsbot/dist/standalone/index.cjs.js';
                  script.async = true;
                  document.body.appendChild(script);
                  window.__cookbook_script = script;
                } catch (e) {
                  console.error("Error while initializing Cookbook:", e);
                }
              };
            </script>
          `,
        ],
      };
    },
  };
};

module.exports = askCookbookPlugin;

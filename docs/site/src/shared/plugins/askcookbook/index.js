
// It is going to be exposed in HTTP requests anyway, so it's fine to just hardcode it here.
const COOKBOOK_PUBLIC_API_KEY =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI2NjU5ODBiNDAwZTliZDQ2MzcwZDlhNzYiLCJpYXQiOjE3MTcxNDE2ODQsImV4cCI6MjAzMjcxNzY4NH0.0JCgi4bJ_f6ILFgEyYAP-KeCm1dzOKwH30tC3jEs2_A";

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
              window.initCookbook = function initCookbook() {
                // It's a public API key, so it's safe to expose it here
                const PUBLIC_API_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI2NzU4Y2YyODM4NTcxODU5MjU0MGIyMDciLCJpYXQiOjE3MzM4NzM0NDgsImV4cCI6MjA0OTQ0OTQ0OH0.Z3cv3HuMkYq3aYZYzCKkkYuM5LI3KG-kuA0R-GaSMV4";

                let cookbookContainer = document.getElementById("__cookbook");
                if (!cookbookContainer) {
                  cookbookContainer = document.createElement("div");
                  cookbookContainer.id = "__cookbook";
                  cookbookContainer.dataset.apiKey = "${COOKBOOK_PUBLIC_API_KEY}";
                  document.body.appendChild(cookbookContainer);
                }

                let cookbookScript = document.getElementById("__cookbook-script");
                if (!cookbookScript) {
                  cookbookScript = document.createElement("script");
                  cookbookScript.id = "__cookbook-script";
                  cookbookScript.src = "https://cdn.jsdelivr.net/npm/@cookbookdev/docsbot/dist/standalone/index.cjs.js";
                  cookbookScript.async = true;
                  document.head.appendChild(cookbookScript);
                }
              }
            </script>
          `,
        ],
      };
    },
  };
}

module.exports = askCookbookPlugin;


import type { LoadContext, Plugin } from "@docusaurus/types";

export type PlausibleOptions = {
  domain: string; // required: yourdomain.com
  apiHost?: string; // optional: e.g. 'https://plausible.io'
  enableInDev?: boolean; // optional: allow tracking when not production
  trackOutboundLinks?: boolean; // optional: default true
  hashMode?: boolean; // optional: track hash-based routing
  trackLocalhost?: boolean; // optional: default false
};

export default function pluginPlausible(
  _context: LoadContext,
  options: PlausibleOptions,
): Plugin {
  const injectJson = JSON.stringify({
    ...options,
    trackOutboundLinks: options.trackOutboundLinks ?? true,
    trackLocalhost: options.trackLocalhost ?? false,
  });

  return {
    name: "docusaurus-plugin-plausible",

    // Expose options to the client via a tiny inline script
    injectHtmlTags() {
      return {
        preBodyTags: [
          {
            tagName: "script",
            attributes: { type: "text/javascript" },
            innerHTML: `window.__PLAUSIBLE_OPTS__ = ${injectJson};`,
          },
        ],
      };
    },

    // Load the client module that wires up tracking
    getClientModules() {
      return [require.resolve("./client")];
    },
  };
}

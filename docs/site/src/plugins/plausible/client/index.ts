// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";

declare global {
  interface Window {
    __PLAUSIBLE_OPTS__?: any;
    __plausible_instance__?: any;
    __plausible_inited__?: boolean;
  }
}

export async function onRouteDidUpdate({ location }: { location: Location }) {
  if (!ExecutionEnvironment.canUseDOM) return;

  const opts = (window as any).__PLAUSIBLE_OPTS__ || {};
  const isProd = process.env.NODE_ENV === "production";
  if (!isProd && !opts.enableInDev) return;

  // Dynamically import the ESM tracker. Different builds expose either a default export
  // (callable factory returning an instance) OR named exports (init/trackPageview/trackEvent).
  const mod: any = await import("@plausible-analytics/tracker");

  // Figure out the correct init function shape
  const init: any = typeof mod.default === "function" ? mod.default : mod.init;

  if (!window.__plausible_inited__) {
    if (typeof init !== "function") {
      console.error(
        "[plausible] init is not a function; module exports:",
        Object.keys(mod),
      );
      return;
    }

    // If default-export style, keep the returned instance. If named-export style,
    // init() usually returns void and we later use mod.trackPageview / mod.trackEvent.
    try {
      const instance = init({
        domain: opts.domain,
        apiHost: opts.apiHost,
        hashMode: !!opts.hashMode,
        trackLocalhost: !!opts.trackLocalhost,
      });
      if (instance) {
        (window as any).__plausible_instance__ = instance;
      }
      window.__plausible_inited__ = true;
    } catch (e) {
      console.error("[plausible] init threw", e);
      return;
    }
  }

  // Resolve a working track function, supporting both APIs
  const track: any =
    (window as any).__plausible_instance__?.track || (mod as any).track;

  if (typeof track !== "function") {
    console.error(
      "[plausible] track is not a function; instance/mod were:",
      (window as any).__plausible_instance__,
      Object.keys(mod),
    );
    return;
  }

  // Pageview on each SPA route change
  track("pageview", {
    url: location.pathname + location.search + location.hash,
    referrer: document.referrer || undefined,
  });
}

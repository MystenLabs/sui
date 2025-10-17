// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";

export async function onRouteDidUpdate({ location }: { location: Location }) {
  if (!ExecutionEnvironment.canUseDOM) return;

  const opts = (window as any).__PLAUSIBLE_OPTS__ || {};
  const isProd = process.env.NODE_ENV === "production";
  if (!isProd && !opts.enableInDev) return;

  // dynamically import ESM module
  const { default: Plausible } = await import("@plausible-analytics/tracker");

  const key = "__plausible_instance__";
  const w = window as any;

  if (!w[key]) {
    w[key] = Plausible({
      domain: opts.domain,
      apiHost: opts.apiHost,
    });
  }

  w[key].trackPageview({
    url: location.pathname + location.search + location.hash,
    referrer: document.referrer || undefined,
  });
}

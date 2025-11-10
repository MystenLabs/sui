// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

declare module "@plausible-analytics/tracker" {
  // Default-export init() returning an instance with trackPageview/trackEvent
  const init: (opts: {
    domain: string;
    apiHost?: string;
    hashMode?: boolean;
    trackLocalhost?: boolean;
  }) => {
    trackPageview: (args?: { url?: string; referrer?: string }) => void;
    trackEvent: (name: string, opts?: { props?: Record<string, any> }) => void;
  };

  export default init;

  // Named-exports API (some builds surface these)
  export function init(
    opts: Parameters<typeof init>[0],
  ): ReturnType<typeof init>;
  export function trackPageview(args?: {
    url?: string;
    referrer?: string;
  }): void;
  export function trackEvent(
    name: string,
    opts?: { props?: Record<string, any> },
  ): void;
}

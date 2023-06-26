// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Plausible from 'plausible-tracker';
import Browser from 'webextension-polyfill';

const WALLET_URL = Browser.runtime.getURL('').slice(0, -1);

const PLAUSIBLE_ENABLED = process.env.NODE_ENV === 'production';

const plausible = Plausible({
	domain: WALLET_URL,
});

// NOTE: Disabled this because it breaks opening new tabs when clicking on anchor elements
// Plausible's outbound link tracking works by inspecting the document, which doesn't exist in every context.
// if (PLAUSIBLE_ENABLED && typeof document !== 'undefined') {
//     plausible.enableAutoOutboundTracking();
// }

export const trackEvent: typeof plausible.trackEvent = (...args) => {
	if (PLAUSIBLE_ENABLED) {
		plausible.trackEvent(...args);
	} else {
		// eslint-disable-next-line no-console
		console.log(`[plausible] Skipping event "${args[0]}" in development.`);
	}
};

export const trackPageview: typeof plausible.trackPageview = ({ url, ...options } = {}) => {
	if (PLAUSIBLE_ENABLED) {
		plausible.trackPageview({
			url: WALLET_URL + url,
			...options,
		});
	} else {
		// eslint-disable-next-line no-console
		console.log(`[plausible] Skipping pageview log "${url}" in development.`);
	}
};

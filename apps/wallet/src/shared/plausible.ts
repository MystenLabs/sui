// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Plausible from 'plausible-tracker';

// NOTE: The url of Sui wallet Chrome extension:
// https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil
const WALLET_URL = 'chrome-extension://opcgpfmipidbgpenhmajoajpbobppdil';

const PLAUSIBLE_ENABLED = process.env.NODE_ENV !== 'development';

const plausible = Plausible({
    domain: WALLET_URL,
});

if (PLAUSIBLE_ENABLED && typeof document !== 'undefined') {
    plausible.enableAutoOutboundTracking();
}

export const trackEvent: typeof plausible.trackEvent = (...args) => {
    if (PLAUSIBLE_ENABLED) {
        plausible.trackEvent(...args);
    } else {
        // eslint-disable-next-line no-console
        console.log(`[plausible] Skipping event "${args[0]}" in development.`);
    }
};

export const trackPageview: typeof plausible.trackPageview = ({
    url,
    ...options
} = {}) => {
    if (PLAUSIBLE_ENABLED) {
        plausible.trackPageview({
            url: WALLET_URL + url,
            ...options,
        });
    } else {
        // eslint-disable-next-line no-console
        console.log(
            `[plausible] Skipping pageview log "${url}" in development.`
        );
    }
};

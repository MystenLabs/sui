// NOTE: The url of Sui wallet Chrome extension:

import Plausible from 'plausible-tracker';

// https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil
const WALLET_URL = 'chrome-extension://opcgpfmipidbgpenhmajoajpbobppdil';

const PLAUSIBLE_ENABLED = process.env.NODE_ENV !== 'development' || true;

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
        console.log(
            `[plausible] Skipping pageview log "${url}" in development.`
        );
    }
};

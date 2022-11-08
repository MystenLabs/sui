// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Metric } from 'web-vitals';

const vitalsUrl = 'https://vitals.vercel-analytics.com/v1/vitals';

function getConnectionSpeed() {
    return (navigator as any)?.connection?.effectiveType;
}

function sendToVercelAnalytics(metric: Metric) {
    const analyticsId = import.meta.env.VITE_VERCEL_ANALYTICS_ID;
    if (!analyticsId) {
        return;
    }

    const body = {
        dsn: analyticsId,
        id: metric.id,
        page: window.location.pathname,
        href: window.location.href,
        event_name: metric.name,
        value: metric.value.toString(),
        speed: getConnectionSpeed(),
    };

    const blob = new Blob([new URLSearchParams(body).toString()], {
        // This content type is necessary for `sendBeacon`
        type: 'application/x-www-form-urlencoded',
    });

    if (navigator.sendBeacon) {
        navigator.sendBeacon(vitalsUrl, blob);
    } else
        fetch(vitalsUrl, {
            body: blob,
            method: 'POST',
            credentials: 'omit',
            keepalive: true,
        });
}

export function reportWebVitals() {
    if (import.meta.env.DEV) {
        console.warn('Skipping web-vitals report in dev.');
        return;
    }

    import('web-vitals').then(({ getCLS, getFID, getFCP, getLCP, getTTFB }) => {
        getCLS(sendToVercelAnalytics);
        getFID(sendToVercelAnalytics);
        getFCP(sendToVercelAnalytics);
        getLCP(sendToVercelAnalytics);
        getTTFB(sendToVercelAnalytics);
    });
}

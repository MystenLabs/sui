// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import { BrowserTracing } from '@sentry/tracing';
import Browser from 'webextension-polyfill';

const WALLET_VERSION = Browser.runtime.getManifest().version;
const SENTRY_DSN =
    'https://e52a4e5c90224fe0800cc96aa2570581@o1314142.ingest.sentry.io/6761112';

const IS_PROD = process.env.NODE_ENV === 'production';

export default function initSentry() {
    if (!IS_PROD) return;

    Sentry.init({
        dsn: SENTRY_DSN,
        integrations: [new BrowserTracing()],
        release: WALLET_VERSION,
        tracesSampleRate: 0.2,
    });
}

// expand this breadcrumb
type Breadcrumbs = {
    type: 'debug';
    category: string;
    message: string;
};

export function addSentryBreadcrumb(breadcrumbs: Breadcrumbs) {
    if (!IS_PROD) return;
    Sentry.addBreadcrumb(breadcrumbs);
}

export function reportSentryError(error: Error) {
    if (!IS_PROD) return;
    Sentry.captureException(error);
}

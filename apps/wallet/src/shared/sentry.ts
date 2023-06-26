// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import Browser from 'webextension-polyfill';

import { growthbook } from '_src/ui/app/experimentation/feature-gating';

const WALLET_VERSION = Browser.runtime.getManifest().version;
const IS_PROD = process.env.NODE_ENV === 'production';

// NOTE: If you want to enable sentry in dev, you can tweak this value:
const ENABLE_SENTRY = IS_PROD;

const SENTRY_DSN = IS_PROD
	? 'https://e52a4e5c90224fe0800cc96aa2570581@o1314142.ingest.sentry.io/6761112'
	: 'https://d1022411f6284cab9660146f3aa514d2@o1314142.ingest.sentry.io/4504697974751232';

export default function initSentry() {
	Sentry.init({
		enabled: ENABLE_SENTRY,
		dsn: SENTRY_DSN,
		integrations: [new Sentry.BrowserTracing()],
		release: WALLET_VERSION,
		tracesSampler: () => {
			if (!IS_PROD) return 1;
			return growthbook.getFeatureValue('wallet-sentry-tracing', 0);
		},
		allowUrls: IS_PROD
			? [
					'ehndjpedolgphielnhnpnkomdhgpaaei', // chrome beta
					'opcgpfmipidbgpenhmajoajpbobppdil', // chrome prod
			  ]
			: undefined,
	});
}

// expand this breadcrumb
type Breadcrumbs = {
	type: 'debug';
	category: string;
	message: string;
};

export function addSentryBreadcrumb(breadcrumbs: Breadcrumbs) {
	Sentry.addBreadcrumb(breadcrumbs);
}

export function reportSentryError(error: Error) {
	Sentry.captureException(error);
}

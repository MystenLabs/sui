// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type BrowserOptions } from '@sentry/browser';
import Browser from 'webextension-polyfill';

const WALLET_VERSION = Browser.runtime.getManifest().version;
const IS_PROD = process.env.NODE_ENV === 'production';

// NOTE: If you want to enable sentry in dev, you can tweak this value:
const ENABLE_SENTRY = IS_PROD;

const SENTRY_DSN = IS_PROD
	? 'https://e52a4e5c90224fe0800cc96aa2570581@o1314142.ingest.sentry.io/6761112'
	: 'https://d1022411f6284cab9660146f3aa514d2@o1314142.ingest.sentry.io/4504697974751232';

export function getSentryConfig({
	integrations,
	tracesSampler,
}: Pick<BrowserOptions, 'integrations' | 'tracesSampler'>): BrowserOptions {
	return {
		enabled: ENABLE_SENTRY,
		dsn: SENTRY_DSN,
		integrations,
		release: WALLET_VERSION,
		sampleRate: 0.05,
		tracesSampler: IS_PROD ? tracesSampler : () => 1,
		allowUrls: IS_PROD
			? [
					'ehndjpedolgphielnhnpnkomdhgpaaei', // chrome beta
					'opcgpfmipidbgpenhmajoajpbobppdil', // chrome prod
				]
			: undefined,
	};
}

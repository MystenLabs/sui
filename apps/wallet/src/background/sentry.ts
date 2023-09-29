// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { growthbook } from '_src/shared/experimentation/features';
import { getSentryConfig } from '_src/shared/sentry-config';
import * as Sentry from '@sentry/browser';

export function initSentry() {
	Sentry.addTracingExtensions();
	Sentry.init(
		getSentryConfig({
			tracesSampler: () => {
				return growthbook.getFeatureValue('wallet-sentry-tracing', 0);
			},
		}),
	);
}

export const captureException = Sentry.captureException;
export const captureMessage = Sentry.captureMessage;

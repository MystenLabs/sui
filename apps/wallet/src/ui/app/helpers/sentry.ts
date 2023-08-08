// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import { getSentryConfig } from '../../../shared/sentry-config';
import { growthbook } from '_src/ui/app/experimentation/feature-gating';

export default function initSentry() {
	Sentry.init(
		getSentryConfig({
			integrations: [new Sentry.BrowserTracing()],
			tracesSampler: () => {
				return growthbook.getFeatureValue('wallet-sentry-tracing', 0);
			},
		}),
	);
}

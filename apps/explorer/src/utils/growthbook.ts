// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBook } from '@growthbook/growthbook';

export const growthbook = new GrowthBook({
	// If you want to develop locally, you can set the API host to this:
	// apiHost: 'http://localhost:3003',
	apiHost: 'https://apps-backend.sui.io',
	clientKey: import.meta.env.PROD ? 'production' : 'development',
	enableDevMode: import.meta.env.DEV,
});

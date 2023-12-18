// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@fontsource-variable/inter';
import '@fontsource-variable/red-hat-mono';
import { RelayEnvironmentProvider } from 'react-relay';
import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { QueryClientProvider } from '@tanstack/react-query';
import React, { Suspense } from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';

import { router } from './pages';
import { initAmplitude } from './utils/analytics/amplitude';
import { growthbook } from './utils/growthbook';
import { queryClient } from './utils/queryClient';
import './utils/sentry';

import '@mysten/dapp-kit/dist/index.css';
import './index.css';
import environment from './utils/environment';

// Load Amplitude as early as we can:
initAmplitude();

// Start loading features as early as we can:
growthbook.loadFeatures();

ReactDOM.createRoot(document.getElementById('root')!).render(
	<React.StrictMode>
		<RelayEnvironmentProvider environment={environment}>
			<GrowthBookProvider growthbook={growthbook}>
				<QueryClientProvider client={queryClient}>
					<RouterProvider router={router} />
				</QueryClientProvider>
			</GrowthBookProvider>
		</RelayEnvironmentProvider>
	</React.StrictMode>,
);

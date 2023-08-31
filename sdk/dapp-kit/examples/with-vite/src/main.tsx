// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import ReactDOM from 'react-dom/client';
import { router } from './routes/index.js';
import { RouterProvider } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { WalletProvider } from '@mysten/dapp-kit';

const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			retry: false,
			refetchOnMount: false,
			refetchInterval: false,
			refetchOnWindowFocus: false,
			refetchIntervalInBackground: false,
		},
	},
});

ReactDOM.createRoot(document.getElementById('root')!).render(
	<React.StrictMode>
		<QueryClientProvider client={queryClient}>
			<WalletProvider enableUnsafeBurner={import.meta.env.DEV} autoConnect>
				<RouterProvider router={router} />
			</WalletProvider>
		</QueryClientProvider>
	</React.StrictMode>,
);

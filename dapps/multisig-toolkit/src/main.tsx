// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import './index.css';
import '@fontsource-variable/inter';
import '@fontsource-variable/red-hat-mono';

import { WalletKitProvider } from '@mysten/wallet-kit';
import { QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';

import { queryClient } from './lib/queryClient';
import { router } from './routes';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
	<React.StrictMode>
		<QueryClientProvider client={queryClient}>
			<WalletKitProvider disableAutoConnect>
				<RouterProvider router={router} />
			</WalletKitProvider>
		</QueryClientProvider>
	</React.StrictMode>,
);

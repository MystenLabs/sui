// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import './index.css';
import '@fontsource/inter/variable.css';
import '@fontsource/red-hat-mono/variable.css';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { WalletKitProvider } from '@mysten/wallet-kit';
import { router } from './routes';
import { RouterProvider } from 'react-router-dom';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from './lib/queryClient';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
	<React.StrictMode>
		<QueryClientProvider client={queryClient}>
			<WalletKitProvider enableUnsafeBurner disableAutoConnect>
				<RouterProvider router={router} />
			</WalletKitProvider>
		</QueryClientProvider>
	</React.StrictMode>,
);

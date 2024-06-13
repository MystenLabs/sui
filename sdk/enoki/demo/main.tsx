// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClientProvider, useSuiClientContext, WalletProvider } from '@mysten/dapp-kit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import ReactDOM from 'react-dom/client';

import '@mysten/dapp-kit/dist/index.css';

import { getFullnodeUrl } from '../../typescript/src/client/network.ts';
import { EnokiWalletProvider } from '../src/react.tsx';
import { App } from './App.tsx';

const queryClient = new QueryClient();

ReactDOM.createRoot(document.getElementById('root')!).render(
	<React.StrictMode>
		<QueryClientProvider client={queryClient}>
			<SuiClientProvider
				networks={{
					testnet: {
						url: getFullnodeUrl('testnet'),
					},
				}}
			>
				<EnokiWalletProvider
					useSuiClientContext={useSuiClientContext as never}
					config={{
						apiKey: 'enoki_public_b995248de4faffd13864f12cd8539a8d',
						clientIds: {
							google: '705781974144-cltddr1ggjnuc3kaimtc881r2n5bderc.apps.googleusercontent.com',
						},
					}}
				>
					<WalletProvider autoConnect={true}>
						<App />
					</WalletProvider>
				</EnokiWalletProvider>
			</SuiClientProvider>
		</QueryClientProvider>
	</React.StrictMode>,
);

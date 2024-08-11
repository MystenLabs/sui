// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createNetworkConfig, SuiClientProvider, WalletProvider } from '@mysten/dapp-kit';
import { getFullnodeUrl } from '@mysten/sui/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from 'react-hot-toast';
import { Outlet } from 'react-router-dom';

import { Header } from './components/Base/Header';
import { KioskClientProvider } from './context/KioskClientContext';

const queryClient = new QueryClient();

const { networkConfig } = createNetworkConfig({
	localnet: { url: getFullnodeUrl('localnet') },
	devnet: { url: getFullnodeUrl('devnet') },
	testnet: { url: getFullnodeUrl('testnet') },
	mainnet: { url: getFullnodeUrl('mainnet') },
});

export default function Root() {
	return (
		<QueryClientProvider client={queryClient}>
			<SuiClientProvider defaultNetwork="testnet" networks={networkConfig}>
				<WalletProvider>
					<KioskClientProvider>
						<Header />
						<div className="min-h-[80vh]">
							<Outlet />
						</div>
						<div className="mt-6 border-t border-primary text-center py-6">
							Copyright © Mysten Labs, Inc.
						</div>
						<Toaster position="bottom-center" />
					</KioskClientProvider>
				</WalletProvider>
			</SuiClientProvider>
		</QueryClientProvider>
	);
}

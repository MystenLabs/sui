// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import type { IdentifierRecord } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { SuiClientProvider, WalletProvider } from 'dapp-kit/src';
import { MockWallet } from './mockWallet.js';
import type { ComponentProps } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

export function createSuiClientContextWrapper(client: SuiClient) {
	return function SuiClientContextWrapper({ children }: { children: React.ReactNode }) {
		return <SuiClientProvider networks={{ test: client }}>{children}</SuiClientProvider>;
	};
}

export function createWalletProviderContextWrapper(
	providerProps: Omit<ComponentProps<typeof WalletProvider>, 'children'> = {},
) {
	const queryClient = new QueryClient();
	return function WalletProviderContextWrapper({ children }: { children: React.ReactNode }) {
		return (
			<SuiClientProvider>
				<QueryClientProvider client={queryClient}>
					<WalletProvider {...providerProps}>{children}</WalletProvider>;
				</QueryClientProvider>
			</SuiClientProvider>
		);
	};
}

export function registerMockWallet(
	walletName: string,
	additionalFeatures: IdentifierRecord<unknown> = {},
) {
	const walletsApi = getWallets();
	const mockWallet = new MockWallet(walletName, additionalFeatures);
	return {
		unregister: walletsApi.register(mockWallet),
		mockWallet,
	};
}

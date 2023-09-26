// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import type { IdentifierRecord, ReadonlyWalletAccount } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { SuiClientProvider } from 'dapp-kit/src';
import { WalletProvider } from 'dapp-kit/src/components/WalletProvider.js';
import type { ComponentProps } from 'react';

import { createMockAccount } from './mocks/mockAccount.js';
import { MockWallet } from './mocks/mockWallet.js';

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

export function registerMockWallet({
	walletName,
	accounts = [createMockAccount()],
	features = {},
}: {
	walletName: string;
	accounts?: ReadonlyWalletAccount[];
	features?: IdentifierRecord<unknown>;
}) {
	const walletsApi = getWallets();
	const mockWallet = new MockWallet(walletName, accounts, features);
	return {
		unregister: walletsApi.register(mockWallet),
		mockWallet,
	};
}

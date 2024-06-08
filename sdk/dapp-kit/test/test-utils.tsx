// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { IdentifierRecord, ReadonlyWalletAccount } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ComponentProps } from 'react';

import { WalletProvider } from '../src/components/WalletProvider.js';
import { SuiClientProvider } from '../src/index.js';
import { createMockAccount } from './mocks/mockAccount.js';
import { MockWallet } from './mocks/mockWallet.js';

export function createSuiClientContextWrapper(client: SuiClient) {
	return function SuiClientContextWrapper({ children }: { children: React.ReactNode }) {
		return <SuiClientProvider networks={{ test: client }}>{children}</SuiClientProvider>;
	};
}

export function createWalletProviderContextWrapper(
	providerProps: Omit<ComponentProps<typeof WalletProvider>, 'children'> = {},
	suiClient: SuiClient = new SuiClient({ url: getFullnodeUrl('localnet') }),
) {
	const queryClient = new QueryClient();
	return function WalletProviderContextWrapper({ children }: { children: React.ReactNode }) {
		return (
			<SuiClientProvider networks={{ test: suiClient }}>
				<QueryClientProvider client={queryClient}>
					<WalletProvider {...providerProps}>{children}</WalletProvider>;
				</QueryClientProvider>
			</SuiClientProvider>
		);
	};
}

export function registerMockWallet({
	id,
	walletName,
	accounts = [createMockAccount()],
	features = {},
}: {
	id?: string | null;
	walletName: string;
	accounts?: ReadonlyWalletAccount[];
	features?: IdentifierRecord<unknown>;
}) {
	const walletsApi = getWallets();
	const mockWallet = new MockWallet(id ?? crypto.randomUUID(), walletName, accounts, features);

	return {
		unregister: walletsApi.register(mockWallet),
		mockWallet,
	};
}

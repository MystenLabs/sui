// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import type { IdentifierRecord, ReadonlyWalletAccount } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { SuiClientProvider } from 'dapp-kit/src';
import { MockWallet } from './mocks/mockWallet.js';
import type { ComponentProps } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createMockAccount } from './mocks/mockAccount.js';
import { DAppKitProvider } from 'dapp-kit/src/components/DAppKitProvider.js';

export function createSuiClientContextWrapper(client: SuiClient) {
	return function SuiClientContextWrapper({ children }: { children: React.ReactNode }) {
		return <SuiClientProvider networks={{ test: client }}>{children}</SuiClientProvider>;
	};
}

export function createDAppKitProviderContextWrapper(
	providerProps: Omit<ComponentProps<typeof DAppKitProvider>, 'children'> = {},
) {
	const queryClient = new QueryClient();
	return function DAppKitProviderContextWrapper({ children }: { children: React.ReactNode }) {
		return (
			<SuiClientProvider>
				<QueryClientProvider client={queryClient}>
					<DAppKitProvider {...providerProps}>{children}</DAppKitProvider>;
				</QueryClientProvider>
			</SuiClientProvider>
		);
	};
}

export function registerMockWallet({
	walletName,
	accounts = [createMockAccount()],
	additionalFeatures = {},
}: {
	walletName: string;
	accounts?: ReadonlyWalletAccount[];
	additionalFeatures?: IdentifierRecord<unknown>;
}) {
	const walletsApi = getWallets();
	const mockWallet = new MockWallet(walletName, accounts, additionalFeatures);
	return {
		unregister: walletsApi.register(mockWallet),
		mockWallet,
	};
}

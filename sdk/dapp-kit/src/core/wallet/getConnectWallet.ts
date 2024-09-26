// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	StandardConnectInput,
	StandardConnectOutput,
	WalletAccount,
	WalletWithRequiredFeatures,
} from '@mysten/wallet-standard';

import type { StoreState } from '../../walletStore.js';

type ConnectWalletArgs = {
	/** The wallet to connect to. */
	wallet: WalletWithRequiredFeatures;

	/** An optional account address to connect to. Defaults to the first authorized account. */
	accountAddress?: string;
} & StandardConnectInput;

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function getConnectWallet(state: StoreState) {
	const { setWalletConnected, setConnectionStatus } = state;

	return async ({
		wallet,
		accountAddress,
		...connectArgs
	}: ConnectWalletArgs): Promise<StandardConnectOutput> => {
		try {
			setConnectionStatus('connecting');

			const connectResult = await wallet.features['standard:connect'].connect(connectArgs);
			const connectedSuiAccounts = connectResult.accounts.filter((account) =>
				account.chains.some((chain) => chain.split(':')[0] === 'sui'),
			);
			const selectedAccount = getSelectedAccount(connectedSuiAccounts, accountAddress);

			setWalletConnected(
				wallet,
				connectedSuiAccounts,
				selectedAccount,
				connectResult.supportedIntents,
			);

			return { accounts: connectedSuiAccounts };
		} catch (error) {
			setConnectionStatus('disconnected');
			throw error;
		}
	};
}

function getSelectedAccount(connectedAccounts: readonly WalletAccount[], accountAddress?: string) {
	if (connectedAccounts.length === 0) {
		return null;
	}

	if (accountAddress) {
		const selectedAccount = connectedAccounts.find((account) => account.address === accountAddress);
		return selectedAccount ?? connectedAccounts[0];
	}

	return connectedAccounts[0];
}

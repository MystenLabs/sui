// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	StandardConnectInput,
	StandardConnectOutput,
	WalletAccount,
	WalletWithRequiredFeatures,
} from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { useWalletStore } from './useWalletStore.js';

type ConnectWalletArgs = {
	/** The wallet to connect to. */
	wallet: WalletWithRequiredFeatures;

	/** An optional account address to connect to. Defaults to the first authorized account. */
	accountAddress?: string;
} & StandardConnectInput;

type ConnectWalletResult = StandardConnectOutput;

type UseConnectWalletMutationOptions = Omit<
	UseMutationOptions<ConnectWalletResult, Error, ConnectWalletArgs, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function useConnectWallet({
	mutationKey,
	...mutationOptions
}: UseConnectWalletMutationOptions = {}) {
	const setWalletConnected = useWalletStore((state) => state.setWalletConnected);
	const setWalletConnectionStatus = useWalletStore((state) => state.setWalletConnectionStatus);
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async (connectWalletArgs) => {
			try {
				const isReconnecting =
					connectWalletArgs.wallet.name === lastConnectedWalletName &&
					connectWalletArgs.accountAddress === lastConnectedAccountAddress;

				setWalletConnectionStatus(isReconnecting ? 'reconnecting' : 'connecting');
				const { connectedSuiAccounts, selectedAccount } = await connectWallet(connectWalletArgs);
				setWalletConnected(connectWalletArgs.wallet, connectedSuiAccounts, selectedAccount);

				return { accounts: connectedSuiAccounts };
			} catch (error) {
				setWalletConnectionStatus('disconnected');
				throw error;
			}
		},
		...mutationOptions,
	});
}

export async function connectWallet({ wallet, accountAddress, ...connectArgs }: ConnectWalletArgs) {
	const connectResult = await wallet.features['standard:connect'].connect(connectArgs);
	const connectedSuiAccounts = connectResult.accounts.filter((account) =>
		account.chains.some((chain) => chain.split(':')[0] === 'sui'),
	);
	const selectedAccount = getSelectedAccount(connectedSuiAccounts, accountAddress);

	return {
		connectedSuiAccounts,
		selectedAccount,
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

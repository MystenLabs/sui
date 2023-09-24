// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import type {
	StandardConnectInput,
	StandardConnectOutput,
	WalletAccount,
	WalletWithRequiredFeatures,
} from '@mysten/wallet-standard';
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

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async ({ wallet, accountAddress, ...standardConnectInput }) => {
			const connectResult = await wallet.features['standard:connect'].connect(standardConnectInput);
			const selectedAccount = getSelectedAccount(connectResult.accounts, accountAddress);

			setWalletConnected(wallet, selectedAccount);
			return connectResult;
		},
		...mutationOptions,
	});
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

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
import { useWalletContext } from '../../components/WalletProvider.js';
import { WalletAlreadyConnectedError } from '../../errors/walletErrors.js';
import { setMostRecentWalletConnectionInfo } from '../../utils/walletUtils.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';

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
	const { currentWallet, storageAdapter, storageKey, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async ({ wallet, accountAddress, ...standardConnectInput }) => {
			if (currentWallet) {
				throw new WalletAlreadyConnectedError(
					currentWallet.name === wallet.name
						? `The user is already connected to wallet ${wallet.name}.`
						: "You must disconnect the wallet you're currently connected to before connecting to a new wallet.",
				);
			}

			dispatch({ type: 'wallet-connection-status-updated', payload: 'connecting' });

			try {
				const connectResult = await wallet.features['standard:connect'].connect(
					standardConnectInput,
				);
				const selectedAccount = getSelectedAccount(connectResult.accounts, accountAddress);

				dispatch({
					type: 'wallet-connected',
					payload: { wallet, currentAccount: selectedAccount },
				});

				await setMostRecentWalletConnectionInfo({
					storageAdapter,
					storageKey,
					walletName: wallet.name,
					accountAddress: selectedAccount?.address,
				});

				return connectResult;
			} catch (error) {
				dispatch({ type: 'wallet-connection-status-updated', payload: 'disconnected' });
				throw error;
			}
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

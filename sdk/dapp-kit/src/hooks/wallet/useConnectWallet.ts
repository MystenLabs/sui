// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import type { StandardConnectInput, StandardConnectOutput } from '@mysten/wallet-standard';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { WalletAlreadyConnectedError, WalletNotFoundError } from 'dapp-kit/src/errors/walletErrors';
import {
	getMostRecentWalletConnectionInfo,
	setMostRecentWalletConnectionInfo,
} from 'dapp-kit/src/components/wallet-provider/walletUtils';
import { walletMutationKeys } from 'dapp-kit/src/constants/walletMutationKeys';

type ConnectWalletArgs = {
	/** The name of the wallet as defined by the wallet standard to connect to. */
	walletName: string;
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
	const { wallets, currentWallet, storageAdapter, storageKey, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async ({ walletName, ...standardConnectInput }) => {
			if (currentWallet) {
				throw new WalletAlreadyConnectedError(
					currentWallet.name === walletName
						? `The user is already connected to wallet ${walletName}.`
						: "You must disconnect the wallet you're currently connected to before connecting to a new wallet.",
				);
			}

			const wallet = wallets.find((wallet) => wallet.name === walletName);
			if (!wallet) {
				throw new WalletNotFoundError(
					`Failed to connect to wallet with name ${walletName}. Double check that the name provided is correct and that a wallet with that name is registered.`,
				);
			}

			dispatch({ type: 'wallet-connection-status-updated', payload: 'connecting' });

			try {
				const connectResult = await wallet.features['standard:connect'].connect(
					standardConnectInput,
				);
				const { walletName: mostRecentWalletName, accountAddress: mostRecentAccountAddress } =
					await getMostRecentWalletConnectionInfo(storageAdapter, storageKey);

				// When connecting to a wallet, we want to connect to the most recently used wallet account if
				// that information is present. This allows for a more intuitive connection experience!
				const hasRecentWalletAccountToConnectTo =
					mostRecentWalletName === wallet.name && !!mostRecentAccountAddress;
				const selectedAccount =
					connectResult.accounts.length > 0 && hasRecentWalletAccountToConnectTo
						? connectResult.accounts.find((account) => account.address === mostRecentAccountAddress)
						: connectResult.accounts[0];

				// A wallet technically doesn't have to authorize any accounts hence the selected account potentially not existing.
				dispatch({
					type: 'wallet-connected',
					payload: { wallet, currentAccount: selectedAccount ?? null },
				});

				await setMostRecentWalletConnectionInfo({
					storageAdapter,
					storageKey,
					walletName,
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

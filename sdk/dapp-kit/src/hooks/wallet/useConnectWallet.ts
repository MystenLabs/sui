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

type ConnectWalletArgs = {
	/** The name of the wallet as defined by the wallet standard to connect to. */
	walletName: string;
} & StandardConnectInput;

type ConnectWalletResult = StandardConnectOutput;

type UseConnectWalletMutationOptions = Omit<
	UseMutationOptions<ConnectWalletResult, Error, ConnectWalletArgs, unknown>,
	'mutationKey' | 'mutationFn'
>;

// TODO: Figure out the query/mutation key story and whether or not we want to expose
// key factories from dapp-kit
function mutationKey(args: Partial<ConnectWalletArgs>) {
	return [{ scope: 'wallet', entity: 'connect-wallet', ...args }] as const;
}

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function useConnectWallet({
	walletName,
	silent,
	...mutationOptions
}: Partial<ConnectWalletArgs> & UseConnectWalletMutationOptions = {}) {
	const { wallets, storageAdapter, storageKey, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: mutationKey({ walletName, silent }),
		mutationFn: async ({ walletName, ...standardConnectInput }) => {
			const wallet = wallets.find((wallet) => wallet.name === walletName);
			if (!wallet) {
				throw new WalletNotFoundError(
					`Failed to connect to wallet with name ${walletName}. Double check that the name provided is correct and that a wallet with that name is registered.`,
				);
			}

			const connectResult = await wallet.features['standard:connect'].connect(standardConnectInput);
			const { walletName: mostRecentWalletName, accountAddress: mostRecentAccountAddress } =
				await getMostRecentWalletConnectionInfo(storageAdapter, storageKey);

			// When connecting to a wallet, we want to connect to the most recently used wallet account if
			// that information is present. This allows for a more intuitive connection experience!
			const selectedAccount =
				mostRecentWalletName === wallet.name
					? connectResult.accounts.find((account) => account.address === mostRecentAccountAddress)
					: connectResult.accounts[0];

			dispatch({
				type: 'wallet-connected',
				payload: { wallet, currentAccount: selectedAccount ?? null },
			});

			await setMostRecentWalletConnectionInfo({
				storageAdapter,
				storageKey,
				walletName,
				accountAddress: selectedAccount?.address ?? 'fixme',
			});

			return connectResult;
		},
		...mutationOptions,
	});
}

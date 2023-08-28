// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import type { StandardConnectInput } from '@mysten/wallet-standard';

type ConnectWalletArgs = {
	walletName: string;
} & StandardConnectInput;

type ConnectWalletResult = {};

type UseConnectWalletMutationOptions = Omit<
	UseMutationOptions<ConnectWalletResult, Error, ConnectWalletArgs>,
	'mutationKey' | 'mutationFn'
>;

export function useConnectWallet({
	walletName,
	silent,
	...mutationOptions
}: UseConnectWalletMutationOptions) {
	return useMutation({
		mutationKey: mutationKey({ walletName, silent }),
		mutationFn: mutationFn({ walletName, silent }),
		...mutationOptions,
	});
}

export function mutationKey(args: ConnectWalletArgs) {
	return [{ scope: 'accounts', entity: 'connect-wallet', ...args }] as const;
}

export function mutationFn({ walletName, ...standardConnectInput }: ConnectWalletArgs) {
	const currentWallet = internalState.wallets.find((wallet) => wallet.name === walletName) ?? null;

	// TODO: Should the current wallet actually be set before we successfully connect to it?
	setState({ currentWallet });

	if (currentWallet) {
		if (walletEventUnsubscribe) {
			walletEventUnsubscribe();
		}
		walletEventUnsubscribe = currentWallet.features['standard:events'].on(
			'change',
			({ accounts, features, chains }) => {
				// TODO: Handle features or chains changing.
				if (accounts) {
					setState({
						accounts,
						currentAccount:
							internalState.currentAccount &&
							!accounts.find(({ address }) => address === internalState.currentAccount?.address)
								? accounts[0]
								: internalState.currentAccount,
					});
				}
			},
		);

		try {
			setState({ status: WalletKitCoreConnectionStatus.CONNECTING });
			await currentWallet.features['standard:connect'].connect(standardConnectInput);
			setState({ status: WalletKitCoreConnectionStatus.CONNECTED });
			try {
				await storageAdapter.set(storageKey, currentWallet.name);
			} catch {
				/* ignore error */
			}

			setState({
				accounts: currentWallet.accounts,
				currentAccount: currentWallet.accounts[0] ?? null,
			});
		} catch (e) {
			console.log('Wallet connection error', e);

			setState({ status: WalletKitCoreConnectionStatus.ERROR });
		}
	} else {
		setState({ status: WalletKitCoreConnectionStatus.DISCONNECTED });
	}
}

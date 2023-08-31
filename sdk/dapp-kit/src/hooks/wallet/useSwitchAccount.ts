// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { setMostRecentWalletConnectionInfo } from 'dapp-kit/src/components/wallet-provider/walletUtils';
import { WalletNotConnectedError, WalletNotFoundError } from 'dapp-kit/src/errors/walletErrors';

type SwitchAccountArgs = {
	accountAddress: string;
};

type SwitchAccountResult = void;

type UseSwitchAccountMutationOptions = Omit<
	UseMutationOptions<SwitchAccountResult, Error, SwitchAccountArgs, unknown>,
	'mutationKey' | 'mutationFn'
>;

// TODO: Figure out the query/mutation key story and whether or not we want to expose
// key factories from dapp-kit
function mutationKey(args: SwitchAccountArgs) {
	return [{ scope: 'wallet', entity: 'switch-account', ...args }] as const;
}

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function useSwitchAccount({
	accountAddress,
	...mutationOptions
}: SwitchAccountArgs & UseSwitchAccountMutationOptions) {
	const { wallets, storageAdapter, storageKey, currentWallet, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: mutationKey({ accountAddress }),
		mutationFn: async ({ accountAddress }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			await setMostRecentWalletConnectionInfo({
				storageAdapter,
				storageKey,
				walletName: currentWallet.name,
				accountAddress: accountAddress,
			});
		},
		...mutationOptions,
	});
}

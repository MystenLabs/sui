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
import { WalletAlreadyConnectedError } from '../../errors/walletErrors.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { useWalletContext } from './useWalletContext.js';

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
	const { currentWallet, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async (connectWalletArgs) => {
			if (currentWallet) {
				throw new WalletAlreadyConnectedError(
					currentWallet.name === connectWalletArgs.wallet.name
						? `The user is already connected to wallet ${connectWalletArgs.wallet.name}.`
						: "You must disconnect the wallet you're currently connected to before connecting to a new wallet.",
				);
			}
			return await connectWallet(dispatch, storageAdapter, storageKey, connectWalletArgs);
		},
		...mutationOptions,
	});
}

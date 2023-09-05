// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignPersonalMessageInput } from '@mysten/wallet-standard';
import type { SuiSignPersonalMessageOutput } from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { walletMutationKeys } from 'dapp-kit/src/constants/walletMutationKeys';
import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors';

type UseSignPersonalMessageArgs = SuiSignPersonalMessageInput;
type UseSignPersonalMessageResult = SuiSignPersonalMessageOutput;

type UseSignPersonalMessageMutationOptions = Omit<
	UseMutationOptions<UseSignPersonalMessageResult, Error, UseSignPersonalMessageArgs, unknown>,
	'mutationFn'
>;
/**
 * Mutation hook for prompting the user to sign a message.
 */
export function useSignPersonalMessage({
	mutationKey,
	...mutationOptions
}: UseSignPersonalMessageMutationOptions) {
	const { currentWallet } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.signPersonalMessage(mutationKey),
		mutationFn: async (personalMessageInput) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signPersonalMessageFeature = currentWallet.features['sui:signPersonalMessage'];
			if (!signPersonalMessageFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signPersonalMessage` feature.",
				);
			}

			return await signPersonalMessageFeature.signPersonalMessage({
				...personalMessageInput,
				account: personalMessageInput.account,
			});
		},
		...mutationOptions,
	});
}

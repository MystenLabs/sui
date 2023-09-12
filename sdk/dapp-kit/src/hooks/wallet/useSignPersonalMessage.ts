// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignPersonalMessageInput } from '@mysten/wallet-standard';
import type { SuiSignPersonalMessageOutput } from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from '../../components/WalletProvider.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../..//errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';

type UseSignPersonalMessageArgs = PartialBy<SuiSignPersonalMessageInput, 'account'>;
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
}: UseSignPersonalMessageMutationOptions = {}) {
	const { currentWallet, currentAccount } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.signPersonalMessage(mutationKey),
		mutationFn: async (signPersonalMessageArgs) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signerAccount = signPersonalMessageArgs.account ?? currentAccount;
			if (!signerAccount) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to sign the personal message with.',
				);
			}

			const signPersonalMessageFeature = currentWallet.features['sui:signPersonalMessage'];
			if (!signPersonalMessageFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signPersonalMessage` feature.",
				);
			}

			return await signPersonalMessageFeature.signPersonalMessage({
				...signPersonalMessageArgs,
				account: signerAccount,
			});
		},
		...mutationOptions,
	});
}

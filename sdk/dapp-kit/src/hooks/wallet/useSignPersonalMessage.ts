// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	SuiSignPersonalMessageInput,
	SuiSignPersonalMessageOutput,
} from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../..//errors/walletErrors.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useCurrentAccount } from './useCurrentAccount.js';
import { useCurrentWallet } from './useCurrentWallet.js';

type UseSignPersonalMessageArgs = PartialBy<SuiSignPersonalMessageInput, 'account'>;

type UseSignPersonalMessageResult = SuiSignPersonalMessageOutput;

type UseSignPersonalMessageError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignPersonalMessageMutationOptions = Omit<
	UseMutationOptions<
		UseSignPersonalMessageResult,
		UseSignPersonalMessageError,
		UseSignPersonalMessageArgs,
		unknown
	>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function useSignPersonalMessage({
	mutationKey,
	...mutationOptions
}: UseSignPersonalMessageMutationOptions = {}) {
	const currentWallet = useCurrentWallet();
	const currentAccount = useCurrentAccount();

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

			const walletFeature = currentWallet.features['sui:signPersonalMessage'];
			if (!walletFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signPersonalMessage` feature.",
				);
			}

			return await walletFeature.signPersonalMessage({
				...signPersonalMessageArgs,
				account: signerAccount,
			});
		},
		...mutationOptions,
	});
}

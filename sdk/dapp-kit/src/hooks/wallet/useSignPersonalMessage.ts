// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	SuiSignPersonalMessageInput,
	SuiSignPersonalMessageOutput,
} from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import type {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../..//errors/walletErrors.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { getSignPersonalMessage } from '../../core/wallet/getSignPersonalMessage.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useWalletStore } from './useWalletStore.js';

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
}: UseSignPersonalMessageMutationOptions = {}): UseMutationResult<
	UseSignPersonalMessageResult,
	UseSignPersonalMessageError,
	UseSignPersonalMessageArgs
> {
	const signPersonalMessage = useWalletStore(getSignPersonalMessage);

	return useMutation({
		mutationKey: walletMutationKeys.signPersonalMessage(mutationKey),
		mutationFn: async (args) => {
			return signPersonalMessage(args);
		},
		...mutationOptions,
	});
}

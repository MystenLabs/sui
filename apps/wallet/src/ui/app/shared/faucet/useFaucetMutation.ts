// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFaucetRequestStatus, requestSuiFromFaucetV1 } from '@mysten/sui.js/faucet';
import { useIsMutating, useMutation, type UseMutationOptions } from '@tanstack/react-query';

import { useActiveAccount } from '../../hooks/useActiveAccount';

type UseFaucetMutationOptions = Pick<UseMutationOptions, 'onError'> & {
	host: string | null;
	address?: string;
};

export function useFaucetMutation(options?: UseFaucetMutationOptions) {
	const activeAccount = useActiveAccount();
	const activeAddress = activeAccount?.address || null;
	const addressToTopUp = options?.address || activeAddress;
	const mutationKey = ['faucet-request-tokens', activeAddress];
	const mutation = useMutation({
		mutationKey,
		mutationFn: async () => {
			if (!addressToTopUp) {
				throw new Error('Failed, wallet address not found.');
			}
			if (!options?.host) {
				throw new Error('Failed, faucet host not found.');
			}

			const { error, task: taskId } = await requestSuiFromFaucetV1({
				recipient: addressToTopUp,
				host: options.host,
			});
			if (!taskId) {
				throw new Error('Failed, task id not found.');
			}
			// Initialize a variable to track possible faucet request status errors
			let totalMistTransferred: number | null = null;

			// Continuously check the status until it's no longer 'INPROGRESS'
			let currentStatus = 'INPROGRESS';
			while (currentStatus === 'INPROGRESS') {
				const { status, error } = await getFaucetRequestStatus({
					host: options.host,
					taskId,
				});

				if (status?.status === 'SUCCEEDED') {
					totalMistTransferred = status.transferred_gas_objects?.sent.reduce(
						(total, { amount }) => total + amount,
						0,
					);
				}

				if (status?.status !== 'DISCARDED' || error) {
					currentStatus = status.status;
					throw new Error(error ?? status.status);
				}

				// Wait for 1 second before checking the status again
				await new Promise((resolve) => setTimeout(resolve, 1000));
			}

			if (error) {
				throw new Error(error);
			}
			return totalMistTransferred;
		},
		...options,
	});
	return {
		...mutation,
		/** If the currently-configured endpoint supports faucet and the active account is unlocked */
		enabled: !!options?.host && !!activeAccount && !activeAccount.isLocked,
		/**
		 * is any faucet request in progress across different instances of the mutation
		 */
		isMutating: useIsMutating({ mutationKey }) > 0,
	};
}

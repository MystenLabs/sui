// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { requestSuiFromFaucetV0 } from '@mysten/sui.js/faucet';
import { useIsMutating, useMutation, type UseMutationOptions } from '@tanstack/react-query';

import { useActiveAddress } from '../../hooks/useActiveAddress';

type UseFaucetMutationOptions = Pick<UseMutationOptions, 'onError'> & {
	host: string | null;
};

export function useFaucetMutation(options?: UseFaucetMutationOptions) {
	const address = useActiveAddress();
	const mutationKey = ['faucet-request-tokens', address];
	const mutation = useMutation({
		mutationKey,
		mutationFn: async () => {
			if (!address) {
				throw new Error('Failed, wallet address not found.');
			}
			if (!options?.host) {
				throw new Error('Failed, faucet host not found.');
			}

			const { error, transferredGasObjects } = await requestSuiFromFaucetV0({
				recipient: address,
				host: options.host,
			});

			if (error) {
				throw new Error(error);
			}
			return transferredGasObjects.reduce((total, { amount }) => total + amount, 0);
		},
		...options,
	});
	return {
		...mutation,
		/** If the currently-configured endpoint supports faucet: */
		enabled: !!options?.host,
		/**
		 * is any faucet request in progress across different instances of the mutation
		 */
		isMutating: useIsMutating({ mutationKey }) > 0,
	};
}

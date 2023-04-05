// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import {
    useIsMutating,
    useMutation,
    type UseMutationOptions,
} from '@tanstack/react-query';

import { useActiveAddress } from '../../hooks/useActiveAddress';

type UseFaucetMutationOptions = Pick<UseMutationOptions, 'onError'>;

export function useFaucetMutation(options?: UseFaucetMutationOptions) {
    const api = useRpcClient();
    const address = useActiveAddress();
    const mutationKey = ['faucet-request-tokens', address];
    const mutation = useMutation({
        mutationKey,
        mutationFn: async () => {
            if (!address) {
                throw new Error('Failed, wallet address not found.');
            }
            const { error, transferredGasObjects } =
                await api.requestSuiFromFaucet(address);
            if (error) {
                throw new Error(error);
            }
            return transferredGasObjects.reduce(
                (total, { amount }) => total + amount,
                0
            );
        },
        ...options,
    });
    return {
        ...mutation,
        /** If the currently-configured endpoint supports faucet: */
        enabled: !!api.connection.faucet,
        /**
         * is any faucet request in progress across different instances of the mutation
         */
        isMutating: useIsMutating({ mutationKey }) > 0,
    };
}

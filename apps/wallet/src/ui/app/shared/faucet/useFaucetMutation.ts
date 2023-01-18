// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useIsMutating, useMutation } from '@tanstack/react-query';

import useAppSelector from '../../hooks/useAppSelector';
import { useRpc } from '../../hooks/useRpc';

export function useFaucetMutation() {
    const api = useRpc();
    const address = useAppSelector(({ account: { address } }) => address);
    const mutationKey = ['faucet-request-tokens', address];
    const mutation = useMutation({
        mutationKey,
        mutationFn: async () => {
            if (!address) {
                throw new Error('Failed, wallet address not found.');
            }
            const { error, transferred_gas_objects } =
                await api.requestSuiFromFaucet(address);
            if (error) {
                throw new Error(error);
            }
            return transferred_gas_objects.reduce(
                (total, { amount }) => total + amount,
                0
            );
        },
    });
    return {
        ...mutation,
        /**
         * is any faucet request in progress across different instances of the mutation
         */
        isMutating: useIsMutating({ mutationKey }) > 0,
    };
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useIsMutating, useMutation } from '@tanstack/react-query';

import useAppSelector from '../../hooks/useAppSelector';
import { useRpc } from '../../hooks/useRpc';

export function useFaucetMutation() {
    const api = useRpc();
    const address = useAppSelector(({ account: { address } }) => address);
    return useMutation({
        mutationKey: ['faucet-request-tokens', address],
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
}

export function useIsFaucetMutating() {
    const address = useAppSelector(({ account: { address } }) => address);
    return (
        useIsMutating({ mutationKey: ['faucet-request-tokens', address] }) > 0
    );
}

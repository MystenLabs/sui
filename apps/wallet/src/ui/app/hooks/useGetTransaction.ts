// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

//TODO use hook useRpc -
import { api } from '../redux/store/thunk-extras';

export function useGetTransaction(transactionId: string) {
    const rpc = api.instance.fullNode;
    return useQuery(
        ['transactions-by-id', transactionId],
        async () => {
            return rpc.getTransactionWithEffects(transactionId);
        },
        { enabled: !!transactionId }
    );
}

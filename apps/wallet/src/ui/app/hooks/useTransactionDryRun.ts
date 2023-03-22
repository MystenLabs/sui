// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress, type Transaction } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useSigner } from '_hooks';

export function useTransactionDryRun(
    sender: SuiAddress | undefined,
    transaction: Transaction
) {
    const signer = useSigner(sender);
    const response = useQuery({
        queryKey: ['dryRunTransaction', transaction.serialize()],
        queryFn: async () => {
            const initializedSigner = await signer();
            return initializedSigner.dryRunTransaction({ transaction });
        },
        enabled: !!signer,
    });
    return response;
}

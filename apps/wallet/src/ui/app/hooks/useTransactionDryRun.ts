// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useSigner } from '_hooks';

import type { SuiAddress, Transaction } from '@mysten/sui.js';

export function useTransactionDryRun(
    sender: SuiAddress,
    transaction: Transaction
) {
    const signer = useSigner(sender);
    const response = useQuery({
        queryKey: ['dryRunTransaction', transaction, sender],
        queryFn: async () => {
            return signer!.dryRunTransaction({ transaction });
        },
        enabled: !!signer,
    });
    return response;
}

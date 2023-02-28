// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useSigner } from '_hooks';

import type { SignerWithProvider, SuiAddress } from '@mysten/sui.js';

export type TransactionDryRun = Parameters<
    SignerWithProvider['dryRunTransaction']
>['0'];

export function useTransactionDryRun(
    txData: TransactionDryRun,
    addressForTransaction: SuiAddress
) {
    const signer = useSigner(addressForTransaction);
    const response = useQuery({
        queryKey: ['executeDryRunTxn', txData, addressForTransaction],
        queryFn: async () => {
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            return signer!.dryRunTransaction(txData);
        },
        enabled: !!signer,
    });
    return response;
}

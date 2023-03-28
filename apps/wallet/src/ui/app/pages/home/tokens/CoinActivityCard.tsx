// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest } from '@mysten/sui.js';
import { useMemo } from 'react';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TransactionCard } from '_components/transactions-card';
import { useQueryTransactionsByAddress } from '_hooks';
import Alert from '_src/ui/app/components/alert';
import { useActiveAddress } from '_src/ui/app/hooks/useActiveAddress';

export function CoinActivitiesCard({ coinType }: { coinType: string }) {
    const activeAddress = useActiveAddress();
    const {
        data: txns,
        isLoading,
        error,
        isError,
    } = useQueryTransactionsByAddress(activeAddress);

    // filter txns by coinType
    const txnByCoinType = useMemo(() => {
        if (!txns || !activeAddress) return null;
        return [];
        // return txns?.filter((txn) => {
        //     const { coins } = getEventsSummary(txn.events!, activeAddress);
        //     // find txn with coinType from eventsSummary
        //     return !!coins.find(
        //         ({ coinType: summaryCoinType }) => summaryCoinType === coinType
        //     );
        // });
    }, [txns, activeAddress]);
    // }, [txns, activeAddress, coinType]);

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="font-semibold">
                        {(error as Error).message}
                    </div>
                </Alert>
            </div>
        );
    }
    return (
        <div className="flex flex-col flex-nowrap ">
            <div className="flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0">
                <Loading loading={isLoading}>
                    {txnByCoinType?.length && activeAddress
                        ? txnByCoinType.map((txn) => (
                              <ErrorBoundary key={getTransactionDigest(txn)}>
                                  <TransactionCard
                                      txn={txn}
                                      address={activeAddress}
                                  />
                              </ErrorBoundary>
                          ))
                        : null}
                </Loading>
            </div>
        </div>
    );
}

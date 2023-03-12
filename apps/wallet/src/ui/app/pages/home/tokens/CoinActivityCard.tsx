// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest } from '@mysten/sui.js';
import { useMemo } from 'react';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TransactionCard } from '_components/transactions-card';
import { getEventsSummary } from '_helpers';
import { useAppSelector, useQueryTransactionsByAddress } from '_hooks';
import Alert from '_src/ui/app/components/alert';

export function CoinActivitiesCard({ coinType }: { coinType: string }) {
    const activeAddress = useAppSelector(({ account: { address } }) => address);
    const {
        data: txns,
        isLoading,
        error,
        isError,
    } = useQueryTransactionsByAddress(activeAddress);

    // filter txns by coinType
    const txnByCoinType = useMemo(() => {
        if (!txns || !activeAddress) return null;
        return txns?.filter((txn) => {
            const { coins } = getEventsSummary(txn.events!, activeAddress);
            // find txn with coinType from eventsSummary
            return !!coins.find(
                ({ coinType: summaryCoinType }) => summaryCoinType === coinType
            );
        });
    }, [txns, activeAddress, coinType]);

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

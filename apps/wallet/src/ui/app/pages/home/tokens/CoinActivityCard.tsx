// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TransactionCard } from '_components/transactions-card';
import { getEventsSummary } from '_helpers';
import { useAppSelector, useGetTransactionsByAddress } from '_hooks';
import Alert from '_src/ui/app/components/alert';

export function CoinActivities({ coinType }: { coinType: string }) {
    const activeAddress = useAppSelector(({ account: { address } }) => address);
    const {
        data: txns,
        isLoading,
        isError,
        error,
    } = useGetTransactionsByAddress(activeAddress);

    // filter txns by coinType
    const txnByCoinType = useMemo(() => {
        if (!txns || !activeAddress) return null;
        return txns?.filter((txn) => {
            const { coins: eventsSummary } = getEventsSummary(
                txn.effects,
                activeAddress
            );

            // find txn with coinType from eventsSummary
            return !!eventsSummary.find(
                ({ coinType: summaryCoinType }) => summaryCoinType === coinType
            );
        });
    }, [txns, activeAddress, coinType]);

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="font-semibold">
                        {error?.message || 'Something went wrong'}
                    </div>
                </Alert>
            </div>
        );
    }
    return (
        <div className="flex flex-col flex-nowrap ">
            <div className="flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0">
                <Loading
                    loading={isLoading}
                    className="flex justify-center items-center h-full"
                >
                    {txnByCoinType?.length && activeAddress
                        ? txnByCoinType.map((txn) => (
                              <ErrorBoundary
                                  key={txn.certificate.transactionDigest}
                              >
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

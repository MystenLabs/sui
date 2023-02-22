// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest } from '@mysten/sui.js';
import { memo } from 'react';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TransactionCard } from '_components/transactions-card';
import { NoActivityCard } from '_components/transactions-card/NoActivityCard';
import { useAppSelector, useGetTransactionsByAddress } from '_hooks';
import Alert from '_src/ui/app/components/alert';
import PageTitle from '_src/ui/app/shared/PageTitle';

function TransactionsPage() {
    const activeAddress = useAppSelector(({ account: { address } }) => address);
    const {
        data: txns,
        isLoading,
        error,
    } = useGetTransactionsByAddress(activeAddress);

    if (error instanceof Error) {
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
        <div className="flex flex-col flex-nowrap h-full overflow-x-visible">
            <PageTitle title="Your Activity" />

            <div className="mt-5 flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0">
                <Loading loading={isLoading}>
                    {txns?.length && activeAddress ? (
                        txns.map((txn) => (
                            <ErrorBoundary key={getTransactionDigest(txn)}>
                                <TransactionCard
                                    txn={txn}
                                    address={activeAddress}
                                />
                            </ErrorBoundary>
                        ))
                    ) : (
                        <NoActivityCard />
                    )}
                </Loading>
            </div>
        </div>
    );
}

export default memo(TransactionsPage);

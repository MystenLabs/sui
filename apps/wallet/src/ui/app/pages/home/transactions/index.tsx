// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest } from '@mysten/sui.js';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TransactionCard } from '_components/transactions-card';
import { NoActivityCard } from '_components/transactions-card/NoActivityCard';
import { useQueryTransactionsByAddress } from '_hooks';
import Alert from '_src/ui/app/components/alert';
import { useActiveAddress } from '_src/ui/app/hooks/useActiveAddress';
import PageTitle from '_src/ui/app/shared/PageTitle';

function TransactionBlocksPage() {
    const activeAddress = useActiveAddress();
    const {
        data: txns,
        isLoading,
        error,
        isError,
    } = useQueryTransactionsByAddress(activeAddress);

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

export default TransactionBlocksPage;

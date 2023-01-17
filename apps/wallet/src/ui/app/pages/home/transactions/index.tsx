// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import { useGetTranactionIdByAddress } from './useGetTransactionByAddress';
import PageTitle from '_app/shared/page-title';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TxnListItem } from '_components/transactions-card/Transaction';
import { useAppSelector } from '_hooks';
import Alert from '_src/ui/app/components/alert';

function TransactionsPage() {
    const activeAddress = useAppSelector(({ account: { address } }) => address);

    const {
        data: txns,
        isError,
        isLoading,
    } = useGetTranactionIdByAddress(activeAddress || '');

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="mb-1 font-semibold">
                        Something went wrong
                    </div>
                </Alert>
            </div>
        );
    }

    return (
        <div className="flex flex-col flex-nowrap h-full overflow-x-visible">
            <PageTitle
                title="Your Activity"
                className="flex justify-center text-heading6 text-gray-90"
            />

            <div className="mt-5 flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0">
                <Loading
                    loading={isLoading}
                    className="flex justify-center items-center"
                >
                    {txns &&
                        txns.map((txnId) => (
                            <ErrorBoundary key={txnId}>
                                <TxnListItem txnId={txnId} />
                            </ErrorBoundary>
                        ))}
                </Loading>
            </div>
        </div>
    );
}

export default memo(TransactionsPage);

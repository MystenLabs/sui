// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { useRecentTransactions } from '../../hooks/useRecentTransactions';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import TransactionCard from '_components/transactions-card';

import st from './TransactionsCard.module.scss';

type Props = {
    coinType?: string;
};

function RecentTransactions({ coinType }: Props) {
    const { isLoading, data } = useRecentTransactions();

    const txByAddress = useMemo(() => {
        if (!data) return [];
        return coinType ? data?.filter((tx) => tx.coinType === coinType) : data;
    }, [data, coinType]);

    return (
        <>
            <Loading loading={isLoading} className={st.centerLoading}>
                {txByAddress.map((txn) => (
                    <ErrorBoundary key={txn.txId}>
                        <TransactionCard txn={txn} />
                    </ErrorBoundary>
                ))}
            </Loading>
        </>
    );
}

export default RecentTransactions;

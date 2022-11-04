// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import TransactionCard from '_components/transactions-card';
import { useAppSelector, useAppDispatch } from '_hooks';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { TxResultState } from '_redux/slices/txresults';

import st from './TransactionsCard.module.scss';

type Props = {
    coinType?: string;
};

function RecentTransactions({ coinType }: Props) {
    const dispatch = useAppDispatch();
    const txByAddress: TxResultState[] = useAppSelector(({ txresults }) =>
        coinType
            ? txresults.latestTx.filter((tx) => tx.coinType === coinType)
            : txresults.latestTx
    );

    const loading: boolean = useAppSelector(
        ({ txresults }) => txresults.loading
    );

    useEffect(() => {
        dispatch(getTransactionsByAddress()).unwrap();
    }, [dispatch]);

    return (
        <>
            <Loading loading={loading} className={st.centerLoading}>
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { memo, useEffect } from 'react';

import PageTitle from '_app/shared/page-title';
import Loading from '_components/loading';
import TransactionCard from '_components/transactions-card';
import { useAppSelector, useAppDispatch } from '_hooks';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { TxResultState } from '_redux/slices/txresults';

import st from './Transactions.module.scss';

function TransactionsPage() {
    const dispatch = useAppDispatch();
    const txByAddress: TxResultState[] = useAppSelector(
        ({ txresults }) => txresults.latestTx
    );

    const loading: boolean = useAppSelector(
        ({ txresults }) => txresults.loading
    );

    useEffect(() => {
        dispatch(getTransactionsByAddress()).unwrap();
    }, [dispatch]);

    return (
        <Loading loading={loading} className={st.centerLoading}>
            {txByAddress && txByAddress.length ? (
                <div className={st.container}>
                    <PageTitle title="Your Activity" />
                    <div className={st.txContent}>
                        {txByAddress.map((txn) => (
                            <TransactionCard txn={txn} key={txn.txId} />
                        ))}
                    </div>
                </div>
            ) : null}
        </Loading>
    );
}

export default memo(TransactionsPage);

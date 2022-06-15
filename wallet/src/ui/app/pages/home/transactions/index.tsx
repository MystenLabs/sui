// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { memo, useEffect } from 'react';

import AccountAddress from '_components/account-address';
import TransactionCard from '_components/transactions-card';
import { useAppSelector, useAppDispatch } from '_hooks';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { TxResultState } from '_redux/slices/txresults';

import st from './Transactions.module.scss';

function TransactionPage() {
    const dispatch = useAppDispatch();
    const txByAddress: TxResultState[] = useAppSelector(
        ({ txresults }) => txresults.latestTx
    );

    useEffect(() => {
        dispatch(getTransactionsByAddress()).unwrap();
    }, [dispatch]);

    return txByAddress && txByAddress.length ? (
        <div className={st.txContainer}>
            <h4>
                Last 5 transaction for <AccountAddress />
            </h4>
            {txByAddress.slice(0, 5).map((txn) => (
                <TransactionCard txn={txn} key={txn.txId} />
            ))}
        </div>
    ) : null;
}

export default memo(TransactionPage);

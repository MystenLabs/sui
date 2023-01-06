// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { memo } from 'react';

import PageTitle from '_app/shared/page-title';
import RecentTransactions from '_components/transactions-card/RecentTransactions';
import { useAppSelector } from '_hooks';

import st from './Transactions.module.scss';

function TransactionsPage() {
    const activeAddress = useAppSelector(({ account: { address } }) => address);
    return (
        <div className={st.container}>
            <PageTitle title="Your Activity" />

            <div className={st.txContent}>
                {activeAddress && (
                    <RecentTransactions address={activeAddress} />
                )}
            </div>
        </div>
    );
}

export default memo(TransactionsPage);

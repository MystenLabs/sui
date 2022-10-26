// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { memo } from 'react';

import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import RecentTransactions from '_components/transactions-card/RecentTransactions';

import st from './Transactions.module.scss';

function TransactionsPage() {
    return (
        <div className={st.container}>
            <PageTitle title="Your Activity" />
            <Content>
                <div className={st.txContent}>
                    <RecentTransactions />
                </div>
            </Content>
        </div>
    );
}

export default memo(TransactionsPage);

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { LatestTxCard } from '../../components/transaction-card/RecentTxCard';
import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './Transactions.module.css';

const TXN_PER_PAGE = 20;
const TRUNCATE_LENGTH = 45;

function TransactionsStatic() {
    return (
        <div data-testid="home-page" id="home" className={styles.home}>
            <LatestTxCard />
        </div>
    );
}

function TransactionsAPI() {
    return (
        <div
            data-testid="transaction-page"
            id="transaction"
            className={styles.container}
        >
            <ErrorBoundary>
                <LatestTxCard
                    txPerPage={TXN_PER_PAGE}
                    paginationtype="pagination"
                    truncateLength={TRUNCATE_LENGTH}
                />
            </ErrorBoundary>
        </div>
    );
}

function Transactions() {
    return IS_STATIC_ENV ? <TransactionsStatic /> : <TransactionsAPI />;
}

export default Transactions;

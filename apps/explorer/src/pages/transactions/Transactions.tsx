// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { LatestTxCard } from '../../components/transaction-card/RecentTxCard';

import styles from './Transactions.module.css';

const TXN_PER_PAGE = 20;
const TRUNCATE_LENGTH = 45;

function Transactions() {
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

export default Transactions;

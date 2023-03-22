// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { LatestTxCard } from '../../components/transaction-card/RecentTxCard';

const TXN_PER_PAGE = 20;

function Transactions() {
    return (
        <div
            data-testid="transaction-page"
            id="transaction"
            className="mx-auto"
        >
            <ErrorBoundary>
                <LatestTxCard
                    txPerPage={TXN_PER_PAGE}
                    paginationtype="pagination"
                />
            </ErrorBoundary>
        </div>
    );
}

export default Transactions;

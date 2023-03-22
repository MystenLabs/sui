// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { LatestTxCard } from '../../components/transaction-card/RecentTxCard';

const TRANSACTIONS_LIMIT = 20;

function Transactions() {
    return (
        <div
            data-testid="transaction-page"
            id="transaction"
            className="mx-auto"
        >
            <ErrorBoundary>
                <LatestTxCard initialLimit={TRANSACTIONS_LIMIT} />
            </ErrorBoundary>
        </div>
    );
}

export default Transactions;

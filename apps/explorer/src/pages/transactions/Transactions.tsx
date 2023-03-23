// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Activity } from '../../components/Activity';
import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';

const TRANSACTIONS_LIMIT = 20;

function Transactions() {
    return (
        <div
            data-testid="transaction-page"
            id="transaction"
            className="mx-auto"
        >
            <ErrorBoundary>
                <Activity initialLimit={TRANSACTIONS_LIMIT} />
            </ErrorBoundary>
        </div>
    );
}

export default Transactions;

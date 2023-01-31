// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionKindName, getMoveCallTransaction } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { CertifiedTransaction } from '@mysten/sui.js';

export function useCallTxnLabel(certificate: CertifiedTransaction) {
    const moveCallLabel = useMemo(() => {
        const txnKind = getTransactionKindName(
            certificate.data.transactions[0]
        );
        if (txnKind !== 'Call') return null;

        const moveCallTxn = getMoveCallTransaction(
            certificate.data.transactions[0]
        );
        if (
            moveCallTxn?.module === 'sui_system' &&
            moveCallTxn?.function === 'request_add_delegation_mul_coin'
        )
            return 'Staked';
        if (
            moveCallTxn?.module === 'sui_system' &&
            moveCallTxn?.function === 'request_withdraw_delegation'
        )
            return 'Unstaked';
        return txnKind;
    }, [certificate]);
    return moveCallLabel;
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { TxnAmount } from '_components/receipt-card/TxnAmount';

import type { SuiTransactionResponse } from '@mysten/sui.js';

type StakeTxnCardProps = {
    txn: SuiTransactionResponse;
    amount: number;
    activeAddress: string;
};

// TODO For unstake Transaction there is no reliable way to get the validator address, reward
// For now show the amount
export function UnStakeTxnCard({ amount }: StakeTxnCardProps) {
    return (
        <>
            {amount && (
                <TxnAmount
                    amount={amount}
                    coinType={SUI_TYPE_ARG}
                    label="Unstake"
                />
            )}
        </>
    );
}

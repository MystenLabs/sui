// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ProgrammableTransaction } from '@mysten/sui.js';

import { InputsCard } from '~/pages/transaction-result/programmable-transaction-view/InputsCard';
import { TransactionsCard } from '~/pages/transaction-result/programmable-transaction-view/TransactionsCard';

interface Props {
    transaction: ProgrammableTransaction;
}

export function ProgrammableTransactionView({ transaction }: Props) {
    return (
        <>
            <InputsCard inputs={transaction.inputs} />
            <TransactionsCard transactions={transaction.transactions} />
        </>
    );
}

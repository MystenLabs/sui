// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTransactionSummary } from '@mysten/core';
import {
    getTransactionKind,
    getTransactionKindName,
    type ProgrammableTransaction,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

import { TransactionDetails } from './TransactionDetails';

import { GasBreakdown } from '~/components/GasBreakdown';
import { InputsCard } from '~/pages/transaction-result/programmable-transaction-view/InputsCard';
import { TransactionsCard } from '~/pages/transaction-result/programmable-transaction-view/TransactionsCard';

interface Props {
    transaction: SuiTransactionBlockResponse;
}

export function TransactionData({ transaction }: Props) {
    const summary = useTransactionSummary({
        transaction,
    });

    const transactionKindName = getTransactionKindName(
        getTransactionKind(transaction)!
    );

    const isProgrammableTransaction =
        transactionKindName === 'ProgrammableTransaction';

    const programmableTxn = transaction.transaction!.data
        .transaction as ProgrammableTransaction;

    return (
        <div className="flex flex-wrap gap-6">
            <section className="flex min-w-[50%] flex-1 flex-col gap-6">
                {transaction.checkpoint && (
                    <TransactionDetails
                        checkpoint={transaction.checkpoint}
                        executedEpoch={transaction.effects?.executedEpoch}
                        sender={summary?.sender}
                        timestamp={transaction.timestampMs}
                    />
                )}

                {isProgrammableTransaction && (
                    <InputsCard inputs={programmableTxn.inputs} />
                )}
            </section>

            <section className="flex flex-1 flex-col gap-6">
                {isProgrammableTransaction && (
                    <>
                        <TransactionsCard
                            transactions={programmableTxn.transactions}
                        />
                        <section data-testid="gas-breakdown">
                            <GasBreakdown summary={summary} />
                        </section>
                    </>
                )}
            </section>
        </div>
    );
}

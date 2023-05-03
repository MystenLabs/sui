// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTransactionSummary } from '@mysten/core';
import {
    getTransactionKind,
    getTransactionKindName,
    type ProgrammableTransaction,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

import { GasBreakdown } from '~/components/GasBreakdown';
import { InputsCard } from '~/pages/transaction-result/programmable-transaction-view/InputsCard';
import { TransactionsCard } from '~/pages/transaction-result/programmable-transaction-view/TransactionsCard';
import { Heading } from '~/ui/Heading';
import { CheckpointSequenceLink } from '~/ui/InternalLink';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

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
            <section className="flex w-96 min-w-[50%] flex-1 flex-col gap-6">
                {transaction.checkpoint && (
                    <TransactionBlockCard>
                        <TransactionBlockCardSection>
                            <div className="flex flex-col gap-2">
                                <Heading
                                    variant="heading4/semibold"
                                    color="steel-darker"
                                >
                                    Checkpoint
                                </Heading>
                                <CheckpointSequenceLink
                                    noTruncate
                                    label={Number(
                                        transaction.checkpoint
                                    ).toLocaleString()}
                                    sequence={transaction.checkpoint}
                                />
                            </div>
                        </TransactionBlockCardSection>
                    </TransactionBlockCard>
                )}

                {isProgrammableTransaction && (
                    <InputsCard inputs={programmableTxn.inputs} />
                )}
            </section>

            <section className="flex w-96 flex-1 flex-col gap-6">
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

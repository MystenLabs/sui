// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin } from '@mysten/core';
import {
    getGasData,
    getTotalGasUsed,
    getTransactionKind,
    getTransactionKindName,
    type ProgrammableTransaction,
    SUI_TYPE_ARG,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

import { InputsCard } from '~/pages/transaction-result/programmable-transaction-view/InputsCard';
import { TransactionsCard } from '~/pages/transaction-result/programmable-transaction-view/TransactionsCard';
import { CopyToClipboard } from '~/ui/CopyToClipboard';
import { DescriptionItem } from '~/ui/DescriptionList';
import { Divider } from '~/ui/Divider';
import { Heading } from '~/ui/Heading';
import { CheckpointSequenceLink, ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

function GasAmount({ amount }: { amount?: bigint | number }) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    return (
        <div className="flex flex-wrap gap-1">
            <div className="flex flex-wrap items-center gap-1">
                <Text variant="pBody/medium" color="steel-darker">
                    {formattedAmount}
                </Text>
                <Text variant="subtitleSmall/medium" color="steel-darker">
                    {symbol}
                </Text>
            </div>

            <div className="flex flex-wrap items-center text-body font-medium text-steel">
                ({amount?.toLocaleString()}
                <div className="ml-0.5 text-subtitleSmall font-medium text-steel">
                    MIST
                </div>
                )
            </div>
        </div>
    );
}

function TotalGasAmount({ amount }: { amount?: bigint | number }) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    return (
        <div className="flex flex-col gap-2">
            <div className="flex items-center gap-0.5">
                <Heading variant="heading3/semibold" color="steel-darker">
                    {formattedAmount}
                </Heading>
                <Text variant="body/medium" color="steel-dark">
                    {symbol}
                </Text>
            </div>

            <div className="flex items-center gap-0.5">
                <Heading variant="heading6/medium" color="steel">
                    {amount?.toLocaleString()}
                </Heading>
                <Text variant="body/medium" color="steel">
                    MIST
                </Text>
            </div>
        </div>
    );
}

interface Props {
    transaction: SuiTransactionBlockResponse;
}

export function TransactionData({ transaction }: Props) {
    const gasData = getGasData(transaction)!;
    const gasPayment = gasData.payment;
    const gasUsed = transaction?.effects!.gasUsed;
    const gasPrice = gasData.price || 1;
    const gasBudget = gasData.budget;

    const transactionKindName = getTransactionKindName(
        getTransactionKind(transaction)!
    );

    const isProgrammableTransaction =
        transactionKindName === 'ProgrammableTransaction';

    const programmableTxn = transaction.transaction!.data
        .transaction as ProgrammableTransaction;

    return (
        <div className="flex flex-wrap gap-6">
            {isProgrammableTransaction && (
                <section className="flex w-96 flex-1 flex-col gap-6">
                    <InputsCard inputs={programmableTxn.inputs} />

                    <TransactionsCard
                        transactions={programmableTxn.transactions}
                    />
                </section>
            )}

            <section className="flex w-96 flex-1 flex-col gap-6">
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
                    <section data-testid="gas-breakdown">
                        <TransactionBlockCard
                            collapsible
                            title={
                                <div className="flex flex-col gap-3">
                                    <Heading
                                        variant="heading4/semibold"
                                        color="steel-darker"
                                    >
                                        Gas & Storage Fee
                                    </Heading>
                                    <TotalGasAmount
                                        amount={getTotalGasUsed(transaction)}
                                    />
                                </div>
                            }
                        >
                            <TransactionBlockCardSection>
                                <div className="flex flex-col gap-3">
                                    <Divider />

                                    <DescriptionItem
                                        title={
                                            <Text variant="pBody/semibold">
                                                Gas Payment
                                            </Text>
                                        }
                                    >
                                        <div className="flex items-center gap-1">
                                            <ObjectLink
                                                // TODO: support multiple gas coins
                                                objectId={
                                                    gasPayment[0].objectId
                                                }
                                            />
                                            <CopyToClipboard
                                                size="sm"
                                                copyText={
                                                    gasPayment[0].objectId
                                                }
                                            />
                                        </div>
                                    </DescriptionItem>
                                    <DescriptionItem
                                        title={
                                            <Text variant="pBody/semibold">
                                                Gas Budget
                                            </Text>
                                        }
                                    >
                                        <GasAmount amount={BigInt(gasBudget)} />
                                    </DescriptionItem>
                                </div>

                                <div className="mt-4 flex flex-col gap-3">
                                    <Divider />

                                    <DescriptionItem
                                        title={
                                            <Text variant="pBody/semibold">
                                                Gas Price
                                            </Text>
                                        }
                                    >
                                        <GasAmount amount={BigInt(gasPrice)} />
                                    </DescriptionItem>
                                    <DescriptionItem
                                        title={
                                            <Text variant="pBody/semibold">
                                                Computation Fee
                                            </Text>
                                        }
                                    >
                                        <GasAmount
                                            amount={Number(
                                                gasUsed?.computationCost
                                            )}
                                        />
                                    </DescriptionItem>

                                    <DescriptionItem
                                        title={
                                            <Text variant="pBody/semibold">
                                                Storage Fee
                                            </Text>
                                        }
                                    >
                                        <GasAmount
                                            amount={Number(
                                                gasUsed?.storageCost
                                            )}
                                        />
                                    </DescriptionItem>

                                    <div className="mt-2 flex flex-col gap-2 rounded-xl border border-dashed border-steel px-4 py-2 md:flex-row md:items-center md:gap-4">
                                        <div className="w-full md:w-40">
                                            <Text
                                                variant="pBody/semibold"
                                                color="steel-darker"
                                            >
                                                Storage Rebate
                                            </Text>
                                        </div>

                                        <div className="ml-0 min-w-0 flex-1 leading-none">
                                            <GasAmount
                                                amount={
                                                    -Number(
                                                        gasUsed?.storageRebate
                                                    )
                                                }
                                            />
                                        </div>
                                    </div>
                                </div>
                            </TransactionBlockCardSection>
                        </TransactionBlockCard>
                    </section>
                )}
            </section>
        </div>
    );
}

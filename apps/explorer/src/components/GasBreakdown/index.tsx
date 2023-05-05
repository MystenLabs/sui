// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    CoinFormat,
    type TransactionSummary,
    useFormatCoin,
} from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { CopyToClipboard } from '~/ui/CopyToClipboard';
import { DescriptionItem } from '~/ui/DescriptionList';
import { Divider } from '~/ui/Divider';
import { Heading } from '~/ui/Heading';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

interface GasProps {
    amount?: bigint | number | string;
}

function GasAmount({ amount }: GasProps) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    if (!amount) {
        return null;
    }

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
                ({BigInt(amount)?.toLocaleString()}
                <div className="ml-0.5 text-subtitleSmall font-medium text-steel">
                    MIST
                </div>
                )
            </div>
        </div>
    );
}

function TotalGasAmount({ amount }: GasProps) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    if (!amount) {
        return null;
    }

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
                    {BigInt(amount)?.toLocaleString()}
                </Heading>
                <Text variant="body/medium" color="steel">
                    MIST
                </Text>
            </div>
        </div>
    );
}

function GasPaymentLinks({ objectIds }: { objectIds: string[] }) {
    return (
        <div className="flex flex-wrap items-center gap-x-4">
            {objectIds.map((objectId, index) => (
                <div key={index} className="flex items-center gap-x-1.5">
                    <ObjectLink objectId={objectId} />
                    <CopyToClipboard size="sm" copyText={objectId} />
                </div>
            ))}
        </div>
    );
}

interface GasBreakdownProps {
    summary?: TransactionSummary;
}

export function GasBreakdown({ summary }: GasBreakdownProps) {
    const gasData = summary?.gas;

    if (!gasData) {
        return null;
    }

    const gasPayment = gasData.payment;
    const gasUsed = gasData.gasUsed;
    const gasPrice = gasData.price || 1;
    const gasBudget = gasData.budget;
    const totalGas = gasData.totalGas;
    const owner = gasData.owner;
    const isSponsored = gasData.isSponsored;

    return (
        <TransactionBlockCard
            collapsible
            title={
                <div className="flex flex-col gap-2">
                    <Heading variant="heading4/semibold" color="steel-darker">
                        Gas & Storage Fee
                    </Heading>
                    <TotalGasAmount amount={totalGas} />
                </div>
            }
        >
            <TransactionBlockCardSection>
                {isSponsored && owner && (
                    <div className="mb-4 flex items-center gap-2 rounded-xl bg-sui/10 px-3 py-2">
                        <Text variant="pBody/medium" color="steel-darker">
                            Paid by
                        </Text>
                        <AddressLink address={owner} />
                    </div>
                )}

                <div className="flex flex-col gap-3">
                    <Divider />

                    <DescriptionItem
                        align="start"
                        title={
                            <Text variant="pBody/semibold">Gas Payment</Text>
                        }
                    >
                        {gasPayment?.length ? (
                            <GasPaymentLinks
                                objectIds={gasPayment.map(
                                    (gas) => gas.objectId
                                )}
                            />
                        ) : null}
                    </DescriptionItem>

                    <DescriptionItem
                        align="start"
                        title={<Text variant="pBody/semibold">Gas Budget</Text>}
                    >
                        {gasBudget ? (
                            <GasAmount amount={BigInt(gasBudget)} />
                        ) : null}
                    </DescriptionItem>
                </div>

                <div className="mt-4 flex flex-col gap-3">
                    <Divider />

                    <DescriptionItem
                        align="start"
                        title={<Text variant="pBody/semibold">Gas Price</Text>}
                    >
                        <GasAmount amount={BigInt(gasPrice)} />
                    </DescriptionItem>
                    <DescriptionItem
                        align="start"
                        title={
                            <Text variant="pBody/semibold">
                                Computation Fee
                            </Text>
                        }
                    >
                        <GasAmount amount={Number(gasUsed?.computationCost)} />
                    </DescriptionItem>

                    <DescriptionItem
                        align="start"
                        title={
                            <Text variant="pBody/semibold">Storage Fee</Text>
                        }
                    >
                        <GasAmount amount={Number(gasUsed?.storageCost)} />
                    </DescriptionItem>

                    <div className="mt-2 flex flex-col gap-2 rounded-xl border border-dashed border-steel px-4 py-2 md:flex-row md:items-center md:gap-4">
                        <div className="w-full md:w-40">
                            <Text variant="pBody/semibold" color="steel-darker">
                                Storage Rebate
                            </Text>
                        </div>

                        <div className="ml-0 min-w-0 flex-1 leading-none">
                            <GasAmount
                                amount={-Number(gasUsed?.storageRebate)}
                            />
                        </div>
                    </div>
                </div>
            </TransactionBlockCardSection>
        </TransactionBlockCard>
    );
}

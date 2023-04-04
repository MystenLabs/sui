// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin, useGetTransferAmount } from '@mysten/core';
import {
    getExecutionStatusError,
    getExecutionStatusType,
    getGasData,
    getTotalGasUsed,
    getTransactionDigest,
    getTransactionKind,
    getTransactionKindName,
    getTransactionSender,
    type ProgrammableTransaction,
    SUI_TYPE_ARG,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { useState } from 'react';

// import {
//     eventToDisplay,
//     getAddressesLinks,
// } from '../../components/events/eventDisplay';

import { Signatures } from './Signatures';
import TxLinks from './TxLinks';

import styles from './TransactionResult.module.css';

import { ProgrammableTransactionView } from '~/pages/transaction-result/programmable-transaction-view';
import { Banner } from '~/ui/Banner';
import { DateCard } from '~/ui/DateCard';
import { DescriptionItem, DescriptionList } from '~/ui/DescriptionList';
import { CheckpointSequenceLink, ObjectLink } from '~/ui/InternalLink';
import { PageHeader } from '~/ui/PageHeader';
import { StatAmount } from '~/ui/StatAmount';
import { TableHeader } from '~/ui/TableHeader';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import { Tooltip } from '~/ui/Tooltip';
import {
    RecipientTransactionAddresses,
    SenderTransactionAddress,
    SponsorTransactionAddress,
} from '~/ui/TransactionAddressSection';
import { ReactComponent as ChevronDownIcon } from '~/ui/icons/chevron_down.svg';

function generateMutatedCreated(tx: SuiTransactionBlockResponse) {
    return [
        ...(tx.effects!.mutated?.length
            ? [
                  {
                      label: 'Updated',
                      links: tx.effects!.mutated.map((item) => item.reference),
                  },
              ]
            : []),
        ...(tx.effects!.created?.length
            ? [
                  {
                      label: 'Created',
                      links: tx.effects!.created?.map((item) => item.reference),
                  },
              ]
            : []),
    ];
}

function GasAmount({
    amount,
    expandable,
    expanded,
}: {
    amount?: bigint | number;
    expandable?: boolean;
    expanded?: boolean;
}) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    return (
        <div className="flex h-full items-center gap-1">
            <div className="flex items-baseline gap-0.5 text-gray-90">
                <Text variant="body/medium">{formattedAmount}</Text>
                <Text variant="subtitleSmall/medium">{symbol}</Text>
            </div>

            <Text variant="bodySmall/medium">
                <div className="flex items-center text-steel">
                    (
                    <div className="flex items-baseline gap-0.5">
                        <div>{amount?.toLocaleString()}</div>
                        <Text variant="subtitleSmall/medium">MIST</Text>
                    </div>
                    )
                </div>
            </Text>

            {expandable && (
                <ChevronDownIcon
                    height={12}
                    width={12}
                    className={clsx('text-steel', expanded && 'rotate-180')}
                />
            )}
        </div>
    );
}

export function TransactionView({
    transaction,
}: {
    transaction: SuiTransactionBlockResponse;
}) {
    const sender = getTransactionSender(transaction)!;
    const gasUsed = transaction?.effects!.gasUsed;

    const [gasFeesExpanded, setGasFeesExpanded] = useState(false);

    const { amount, coinType, balanceChanges } =
        useGetTransferAmount(transaction);

    const [formattedAmount, symbol] = useFormatCoin(amount, coinType);

    // const txKindData = formatByTransactionKind(txKindName, txnDetails, sender);
    // const txEventData = transaction.events?.map(eventToDisplay);

    // MUSTFIX(chris): re-enable event display
    // let eventTitles: [string, string][] = [];
    // const txEventDisplay = txEventData?.map((ed, index) => {
    //     if (!ed) return <div />;

    //     let key = ed.top.title + index;
    //     eventTitles.push([ed.top.title, key]);
    //     return (
    //         <div className={styles.txgridcomponent} key={key}>
    //             <ItemView data={ed.top as TxItemView} />
    //             {ed.fields && <ItemView data={ed.fields as TxItemView} />}
    //         </div>
    //     );
    // });

    // let eventTitlesDisplay = eventTitles.map(([title, key]) => (
    //     <div key={key} className={styles.eventtitle}>
    //         {title}
    //     </div>
    // ));

    const createdMutateData = generateMutatedCreated(transaction);

    // MUSTFIX(chris): re-enable event display
    // const hasEvents = txEventData && txEventData.length > 0;
    const hasEvents = false;

    const txError = getExecutionStatusError(transaction);

    const gasData = getGasData(transaction)!;
    const gasPrice = gasData.price || 1;
    const gasPayment = gasData.payment;
    const gasBudget = gasData.budget;
    const gasOwner = gasData.owner;
    const isSponsoredTransaction = gasOwner !== sender;

    const timestamp = transaction.timestampMs;
    const transactionKindName = getTransactionKindName(
        getTransactionKind(transaction)!
    );

    return (
        <div className={clsx(styles.txdetailsbg)}>
            <div className="mb-10">
                <PageHeader
                    type="Transaction"
                    title={getTransactionDigest(transaction)}
                    subtitle={
                        transactionKindName !== 'ProgrammableTransaction'
                            ? transactionKindName
                            : undefined
                    }
                    status={getExecutionStatusType(transaction)}
                />
                {txError && (
                    <div className="mt-2">
                        <Banner variant="error">{txError}</Banner>
                    </div>
                )}
            </div>
            <TabGroup size="lg">
                <TabList>
                    <Tab>Details</Tab>
                    {hasEvents && <Tab>Events</Tab>}
                    <Tab>Signatures</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        <div
                            className={styles.txgridcomponent}
                            // TODO: Change to test ID
                            id={getTransactionDigest(transaction)}
                        >
                            <section
                                className={clsx([
                                    styles.txcomponent,
                                    styles.txsender,
                                    'md:ml-4',
                                ])}
                                data-testid="transaction-timestamp"
                            >
                                {coinType && formattedAmount ? (
                                    <section className="mb-10">
                                        <StatAmount
                                            amount={formattedAmount}
                                            symbol={symbol}
                                            date={+(timestamp ?? 0)}
                                        />
                                    </section>
                                ) : (
                                    timestamp && (
                                        <div className="mb-3">
                                            <DateCard
                                                date={+(timestamp ?? 0)}
                                            />
                                        </div>
                                    )
                                )}
                                {isSponsoredTransaction && (
                                    <div className="mt-10">
                                        <SponsorTransactionAddress
                                            sponsor={gasOwner}
                                        />
                                    </div>
                                )}
                                <div className="mt-10">
                                    <SenderTransactionAddress sender={sender} />
                                </div>
                                {balanceChanges.length > 0 && (
                                    <div className="mt-10">
                                        <RecipientTransactionAddresses
                                            recipients={balanceChanges}
                                        />
                                    </div>
                                )}
                            </section>

                            <section
                                className={clsx([
                                    styles.txcomponent,
                                    styles.txgridcolspan2,
                                ])}
                            >
                                <div className={styles.txlinks}>
                                    {createdMutateData.map((item, idx) => (
                                        <TxLinks data={item} key={idx} />
                                    ))}
                                </div>
                            </section>
                        </div>

                        {transactionKindName === 'ProgrammableTransaction' && (
                            <ProgrammableTransactionView
                                transaction={
                                    transaction.transaction!.data
                                        .transaction as ProgrammableTransaction
                                }
                            />
                        )}

                        {transaction.checkpoint && (
                            <section className="py-12">
                                <TableHeader>Checkpoint Detail</TableHeader>
                                <div className="pt-4">
                                    <DescriptionItem title="Checkpoint Seq. Number">
                                        <CheckpointSequenceLink
                                            noTruncate
                                            sequence={String(
                                                transaction.checkpoint
                                            )}
                                        />
                                    </DescriptionItem>
                                </div>
                            </section>
                        )}

                        <div data-testid="gas-breakdown" className="mt-8">
                            <TableHeader
                                subText={
                                    isSponsoredTransaction
                                        ? '(Paid by Sponsor)'
                                        : undefined
                                }
                            >
                                Gas & Storage Fees
                            </TableHeader>

                            <DescriptionList>
                                <DescriptionItem title="Gas Payment">
                                    <ObjectLink
                                        // TODO: support multiple gas coins
                                        objectId={gasPayment[0].objectId}
                                    />
                                </DescriptionItem>

                                <DescriptionItem title="Gas Budget">
                                    <GasAmount amount={BigInt(gasBudget)} />
                                </DescriptionItem>

                                {gasFeesExpanded && (
                                    <>
                                        <DescriptionItem title="Gas Price">
                                            <GasAmount
                                                amount={BigInt(gasPrice)}
                                            />
                                        </DescriptionItem>
                                        <DescriptionItem title="Computation Fee">
                                            <GasAmount
                                                amount={Number(
                                                    gasUsed?.computationCost
                                                )}
                                            />
                                        </DescriptionItem>

                                        <DescriptionItem title="Storage Fee">
                                            <GasAmount
                                                amount={Number(
                                                    gasUsed?.storageCost
                                                )}
                                            />
                                        </DescriptionItem>

                                        <DescriptionItem title="Storage Rebate">
                                            <GasAmount
                                                amount={Number(
                                                    gasUsed?.storageRebate
                                                )}
                                            />
                                        </DescriptionItem>

                                        <div className="h-px bg-gray-45" />
                                    </>
                                )}

                                <DescriptionItem
                                    title={
                                        <Text
                                            variant="body/semibold"
                                            color="steel-darker"
                                        >
                                            Total Gas Fee
                                        </Text>
                                    }
                                >
                                    <Tooltip
                                        tip={
                                            gasFeesExpanded
                                                ? 'Hide Gas Fee breakdown'
                                                : 'Show Gas Fee breakdown'
                                        }
                                    >
                                        <button
                                            className="cursor-pointer border-none bg-inherit p-0"
                                            type="button"
                                            onClick={() =>
                                                setGasFeesExpanded(
                                                    (expanded) => !expanded
                                                )
                                            }
                                        >
                                            <GasAmount
                                                amount={getTotalGasUsed(
                                                    transaction
                                                )}
                                                expanded={gasFeesExpanded}
                                                expandable
                                            />
                                        </button>
                                    </Tooltip>
                                </DescriptionItem>
                            </DescriptionList>
                        </div>
                    </TabPanel>
                    {/* {hasEvents && (
                        <TabPanel>
                            <div className={styles.txevents}>
                                <div className={styles.txeventsleft}>
                                    {eventTitlesDisplay}
                                </div>
                                <div className={styles.txeventsright}>
                                    {txEventDisplay}
                                </div>
                            </div>
                        </TabPanel>
                    )} */}
                    <TabPanel>
                        <Signatures transaction={transaction} />
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}

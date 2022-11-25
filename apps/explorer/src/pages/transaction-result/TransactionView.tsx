// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getMoveCallTransaction,
    getPublishTransaction,
    getTransactionKindName,
    getTransactions,
    getTransactionSender,
    getTransferObjectTransaction,
    getMovePackageContent,
    getObjectId,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import cl from 'clsx';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import {
    eventToDisplay,
    getAddressesLinks,
} from '../../components/events/eventDisplay';
import Longtext from '../../components/longtext/Longtext';
import ModulesWrapper from '../../components/module/ModulesWrapper';
import {
    type LinkObj,
    TxAddresses,
} from '../../components/transaction-card/TxCardUtils';
import { getAmount } from '../../utils/getAmount';
import TxLinks from './TxLinks';

import type { DataType, Category } from './TransactionResultType';
import type {
    CertifiedTransaction,
    TransactionKindName,
    ExecutionStatusType,
    SuiTransactionKind,
    SuiObjectRef,
    SuiEvent,
} from '@mysten/sui.js';
import type { ReactNode } from 'react';

import styles from './TransactionResult.module.css';

import { CoinFormat, useFormatCoin } from '~/hooks/useFormatCoin';
import { Banner } from '~/ui/Banner';
import { DateCard } from '~/ui/DateCard';
import { PageHeader } from '~/ui/PageHeader';
import { SenderRecipient } from '~/ui/SenderRecipient';
import { StatAmount } from '~/ui/StatAmount';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';

type TxDataProps = CertifiedTransaction & {
    status: ExecutionStatusType;
    timestamp_ms: number | null;
    gasFee: number;
    txError: string;
    mutated: SuiObjectRef[];
    created: SuiObjectRef[];
    events?: SuiEvent[];
};

function generateMutatedCreated(tx: TxDataProps) {
    return [
        ...(tx.mutated?.length
            ? [
                  {
                      label: 'Updated',
                      links: tx.mutated,
                  },
              ]
            : []),
        ...(tx.created?.length
            ? [
                  {
                      label: 'Created',
                      links: tx.created,
                  },
              ]
            : []),
    ];
}

function formatByTransactionKind(
    kind: TransactionKindName | undefined,
    data: SuiTransactionKind,
    sender: string
) {
    switch (kind) {
        case 'TransferObject':
            const transfer = getTransferObjectTransaction(data)!;
            return {
                title: 'Transfer',
                sender: {
                    value: sender,
                    link: true,
                    category: 'addresses',
                },
                objectId: {
                    value: transfer.objectRef.objectId,
                    link: true,
                    category: 'objects',
                },
                recipient: {
                    value: transfer.recipient,
                    category: 'addresses',
                    link: true,
                },
            };
        case 'Call':
            const moveCall = getMoveCallTransaction(data)!;
            return {
                title: 'Call',
                sender: {
                    value: sender,
                    link: true,
                    category: 'addresses',
                },
                package: {
                    value: getObjectId(moveCall.package),
                    link: true,
                    category: 'objects',
                },
                module: {
                    value: moveCall.module,
                },
                function: {
                    value: moveCall.function,
                },
                arguments: {
                    value: moveCall.arguments,
                    list: true,
                },
                typeArguments: {
                    value: moveCall.typeArguments,
                    list: true,
                },
            };
        case 'Publish':
            const publish = getPublishTransaction(data)!;
            return {
                title: 'publish',
                module: {
                    value: Object.entries(getMovePackageContent(publish)!),
                },
                ...(sender
                    ? {
                          sender: {
                              value: sender,
                              link: true,
                              category: 'addresses',
                          },
                      }
                    : {}),
            };

        default:
            return {};
    }
}

type TxItemView = {
    title: string;
    titleStyle?: string;
    content: {
        label?: string | number | any;
        value: ReactNode;
        link?: boolean;
        category?: string;
        monotypeClass?: boolean;
        href?: string;
    }[];
};

function ItemView({ data }: { data: TxItemView }) {
    return (
        <div className={styles.itemView}>
            <div
                className={
                    data.titleStyle
                        ? styles[data.titleStyle]
                        : styles.itemviewtitle
                }
            >
                {data.title}
            </div>
            <div className={styles.itemviewcontent}>
                {data.content.map((item, index) => {
                    // handle sender -> recipient display in one line
                    let links: LinkObj[] = [];
                    let label = item.label;
                    if (Array.isArray(item)) {
                        links = getAddressesLinks(item);
                        label = 'Sender, Recipient';
                    }

                    return (
                        <div
                            key={index}
                            className={cl(
                                styles.itemviewcontentitem,
                                label && styles.singleitem
                            )}
                        >
                            {label && (
                                <div className={styles.itemviewcontentlabel}>
                                    {label}
                                </div>
                            )}
                            <div
                                className={cl(
                                    styles.itemviewcontentvalue,
                                    item.monotypeClass && styles.mono
                                )}
                            >
                                {links.length > 1 && (
                                    <TxAddresses content={links} />
                                )}
                                {item.link ? (
                                    <Longtext
                                        text={item.value as string}
                                        category={item.category as Category}
                                        isLink
                                        copyButton="16"
                                    />
                                ) : item.href ? (
                                    <LinkWithQuery
                                        to={item.href}
                                        className={styles.customhreflink}
                                    >
                                        {item.value}
                                    </LinkWithQuery>
                                ) : (
                                    item.value
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

function GasAmount({ amount }: { amount: bigint | number }) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    return (
        <div className="flex items-center gap-1 h-full">
            <div className="text-gray-90 flex items-baseline gap-0.5">
                <Text variant="body">{formattedAmount}</Text>
                <Text variant="subtitleSmall">{symbol}</Text>
            </div>
            <Text variant="bodySmall">
                <div className="text-gray-65 flex items-center">
                    (
                    <div className="flex items-baseline gap-0.5">
                        <div>{amount.toLocaleString()}</div>
                        <Text variant="subtitleSmall">MIST</Text>
                    </div>
                    )
                </div>
            </Text>
        </div>
    );
}

function TransactionView({ txdata }: { txdata: DataType }) {
    const txdetails = getTransactions(txdata)[0];
    const txKindName = getTransactionKindName(txdetails);
    const sender = getTransactionSender(txdata);

    const txnTransfer = getAmount(txdetails, txdata.transaction?.effects);
    const sendReceiveRecipients = txnTransfer?.map((item) => ({
        address: item.recipientAddress,
        ...(item?.amount
            ? {
                  coin: {
                      amount: item.amount,
                      coinType: item?.coinType || null,
                  },
              }
            : {}),
    }));

    const [formattedAmount, symbol] = useFormatCoin(
        txnTransfer?.[0].amount,
        txnTransfer?.[0].coinType
    );

    const txKindData = formatByTransactionKind(txKindName, txdetails, sender);

    const txEventData = txdata.events?.map(eventToDisplay);

    let eventTitles: [string, string][] = [];
    const txEventDisplay = txEventData?.map((ed, index) => {
        if (!ed) return <div />;

        let key = ed.top.title + index;
        eventTitles.push([ed.top.title, key]);
        return (
            <div className={styles.txgridcomponent} key={key}>
                <ItemView data={ed.top as TxItemView} />
                {ed.fields && <ItemView data={ed.fields as TxItemView} />}
            </div>
        );
    });

    let eventTitlesDisplay = eventTitles.map((et) => (
        <div key={et[1]} className={styles.eventtitle}>
            <Longtext text={et[0]} category="unknown" isLink={false} />
        </div>
    ));

    const transactionSignatureData = {
        title: 'Transaction Signatures',
        content: [
            {
                label: 'Signature',
                value: txdata.txSignature,
                monotypeClass: true,
            },
        ],
    };

    let validatorSignatureData;
    if (Array.isArray(txdata.authSignInfo.signature)) {
        validatorSignatureData = {
            title: 'Validator Signatures',
            content: txdata.authSignInfo.signature.map(
                (validatorSign, index) => ({
                    label: `Signature #${index + 1}`,
                    value: validatorSign,
                    monotypeClass: true,
                })
            ),
        };
    } else {
        validatorSignatureData = {
            title: 'Aggregated Validator Signature',
            content: [
                {
                    label: `Signature`,
                    value: txdata.authSignInfo.signature,
                    monotypeClass: true,
                },
            ],
        };
    }

    const createdMutateData = generateMutatedCreated(txdata);

    const GasStorageFees = {
        title: 'Gas & Storage Fees',
        content: [
            {
                label: 'Gas Payment',
                value: txdata.data.gasPayment.objectId,
                link: true,
            },
            {
                label: 'Gas Budget',
                value: <GasAmount amount={txdata.data.gasBudget} />,
            },
            {
                label: 'Total Gas Fee',
                value: <GasAmount amount={txdata.gasFee} />,
            },
            //TODO: Add Storage Fees
        ],
    };
    const typearguments =
        txKindData.title === 'Call' && txKindData.package
            ? {
                  title: 'Package Details',
                  content: [
                      {
                          label: 'Package ID',
                          monotypeClass: true,
                          link: true,
                          category: 'objects',
                          value: txKindData.package.value,
                      },
                      {
                          label: 'Module',
                          monotypeClass: true,
                          value: txKindData.module.value,
                          href: `/objects/${txKindData.package.value}?module=${txKindData.module.value}`,
                      },
                      {
                          label: 'Function',
                          monotypeClass: true,
                          value: txKindData.function.value,
                      },
                      {
                          label: 'Argument',
                          monotypeClass: true,
                          value: JSON.stringify(txKindData.arguments.value),
                      },
                  ],
              }
            : false;

    if (typearguments && txKindData.typeArguments?.value) {
        typearguments.content.push({
            label: 'Type Arguments',
            monotypeClass: true,
            value: JSON.stringify(txKindData.typeArguments.value),
        });
    }

    const modules =
        txKindData?.module?.value && Array.isArray(txKindData?.module?.value)
            ? {
                  title: 'Modules',
                  content: txKindData?.module?.value,
              }
            : false;

    const hasEvents = txEventData && txEventData.length > 0;

    return (
        <div className={cl(styles.txdetailsbg)}>
            <div className="mt-5 mb-10">
                <PageHeader
                    type={txKindName}
                    title={txdata.txId}
                    status={txdata.status}
                />
                {txdata.txError && (
                    <div className="mt-2">
                        <Banner variant="error">{txdata.txError}</Banner>
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
                            id={txdata.txId}
                        >
                            {typearguments && (
                                <section
                                    className={cl([
                                        styles.txcomponent,
                                        styles.txgridcolspan2,
                                        styles.packagedetails,
                                    ])}
                                >
                                    <ItemView data={typearguments} />
                                </section>
                            )}
                            <section
                                className={cl([
                                    styles.txcomponent,
                                    styles.txsender,
                                    'md:ml-4',
                                ])}
                                data-testid="transaction-timestamp"
                            >
                                {txnTransfer?.[0].amount ? (
                                    <section className="mb-10">
                                        <StatAmount
                                            amount={formattedAmount}
                                            symbol={symbol}
                                            date={txdata.timestamp_ms}
                                        />
                                    </section>
                                ) : (
                                    txdata.timestamp_ms && (
                                        <div className="mb-3">
                                            <DateCard
                                                date={txdata.timestamp_ms}
                                            />
                                        </div>
                                    )
                                )}
                                <SenderRecipient
                                    sender={sender}
                                    transferCoin={txnTransfer?.[0].isCoin}
                                    recipients={sendReceiveRecipients}
                                />
                            </section>

                            <section
                                className={cl([
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

                            {modules && (
                                <section
                                    className={cl([
                                        styles.txcomponent,
                                        styles.txgridcolspan3,
                                    ])}
                                >
                                    <ErrorBoundary>
                                        <ModulesWrapper
                                            id={txKindData.objectId?.value}
                                            data={modules}
                                        />
                                    </ErrorBoundary>
                                </section>
                            )}
                        </div>
                        <div className={styles.txgridcomponent}>
                            <ItemView data={GasStorageFees} />
                        </div>
                    </TabPanel>
                    {hasEvents && (
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
                    )}
                    <TabPanel>
                        <div className={styles.txgridcomponent}>
                            <ItemView data={transactionSignatureData} />
                            <ItemView data={validatorSignatureData} />
                        </div>
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}

export default TransactionView;

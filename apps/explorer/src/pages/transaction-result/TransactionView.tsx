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
    getTransferSuiTransaction,
    getTransferSuiAmount,
} from '@mysten/sui.js';
import cl from 'clsx';
import { Link } from 'react-router-dom';

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
import { presentBN } from '../../utils/stringUtils';
import SendReceiveView from './SendReceiveView';
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

import styles from './TransactionResult.module.css';

import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

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
                      links: tx.mutated.map((obj) => obj.objectId),
                  },
              ]
            : []),
        ...(tx.created?.length
            ? [
                  {
                      label: 'Created',
                      links: tx.created.map((obj) => obj.objectId),
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
        value: string | number;
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
                                    <TxAddresses content={links}></TxAddresses>
                                )}
                                {item.link ? (
                                    <Longtext
                                        text={item.value as string}
                                        category={item.category as Category}
                                        isLink={true}
                                        copyButton="16"
                                    />
                                ) : item.href ? (
                                    <Link
                                        to={item.href}
                                        className={styles.customhreflink}
                                    >
                                        {item.value}
                                    </Link>
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

function TransactionView({ txdata }: { txdata: DataType }) {
    const txdetails = getTransactions(txdata)[0];
    const amount = getTransferSuiAmount(txdetails);
    const txKindName = getTransactionKindName(txdetails);
    const sender = getTransactionSender(txdata);
    const recipient =
        getTransferObjectTransaction(txdetails) ||
        getTransferSuiTransaction(txdetails);
    const txKindData = formatByTransactionKind(txKindName, txdetails, sender);

    const txEventData = txdata.events?.map(eventToDisplay);

    let eventTitles: [string, string][] = [];
    const txEventDisplay = txEventData?.map((ed, index) => {
        if (!ed) return <div></div>;

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
            <Longtext text={et[0]} category={'unknown'} isLink={false} />
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

    const sendreceive = {
        sender: sender,
        ...(txdata.timestamp_ms
            ? {
                  timestamp_ms: txdata.timestamp_ms,
              }
            : {}),
        recipient: [...(recipient?.recipient ? [recipient.recipient] : [])],
    };
    const GasStorageFees = {
        title: 'Gas & Storage Fees',
        content: [
            {
                label: 'Gas Payment',
                value: txdata.data.gasPayment.objectId,
                link: true,
            },
            {
                label: 'Gas Fees',
                value: txdata.gasFee,
            },
            {
                label: 'Gas Budget',
                value: txdata.data.gasBudget,
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
                                ])}
                            >
                                {amount !== null && (
                                    <div className={styles.amountbox}>
                                        <div>Amount</div>
                                        <div>
                                            {presentBN(amount)}
                                            <sup>SUI</sup>
                                        </div>
                                    </div>
                                )}
                                <div className={styles.txaddress}>
                                    <SendReceiveView data={sendreceive} />
                                </div>
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
                                    <ModulesWrapper
                                        id={txKindData.objectId?.value}
                                        data={modules}
                                    />
                                </section>
                            )}
                        </div>
                        <div className={styles.txgridcomponent}>
                            <ItemView data={GasStorageFees} />
                        </div>
                    </TabPanel>
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

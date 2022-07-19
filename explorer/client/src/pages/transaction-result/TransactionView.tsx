// Copyright (c) 2022, Mysten Labs, Inc.
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
} from '@mysten/sui.js';
import cl from 'classnames';

import Longtext from '../../components/longtext/Longtext';
import Tabs from '../../components/tabs/Tabs';
import SendReceiveView from './SendReceiveView';
import TxLinks from './TxLinks';
import TxModulesWrapper from './TxModulesWrapper';
import TxResultHeader from './TxResultHeader';

import type { DataType, Category } from './TransactionResultType';
import type {
    CertifiedTransaction,
    TransactionKindName,
    ExecutionStatusType,
    SuiTransactionKind,
    SuiObjectRef,
} from '@mysten/sui.js';

import styles from './TransactionResult.module.css';

type TxDataProps = CertifiedTransaction & {
    status: ExecutionStatusType;
    timestamp_ms: number | null;
    gasFee: number;
    txError: string;
    mutated: SuiObjectRef[];
    created: SuiObjectRef[];
};

function generateMutatedCreated(tx: TxDataProps) {
    return [
        ...(tx.mutated?.length
            ? [
                  {
                      label: 'Mutated',
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
    content: {
        label?: string | number | any;
        value: string | number;
        link?: boolean;
        category?: string;
        monotypeClass?: boolean;
    }[];
};

function ItemView({ data }: { data: TxItemView }) {
    return (
        <div className={styles.itemView}>
            <div className={styles.itemviewtitle}>{data.title}</div>
            <div className={styles.itemviewcontent}>
                {data.content.map((item, index) => {
                    return (
                        <div
                            key={index}
                            className={cl(
                                styles.itemviewcontentitem,
                                !item.label && styles.singleitem
                            )}
                        >
                            {item.label && (
                                <div className={styles.itemviewcontentlabel}>
                                    {item.label}
                                </div>
                            )}
                            <div
                                className={cl(
                                    styles.itemviewcontentvalue,
                                    item.monotypeClass && styles.mono
                                )}
                            >
                                {item.link ? (
                                    <Longtext
                                        text={item.value as string}
                                        category={item.category as Category}
                                        isLink={true}
                                    />
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
    const txKindName = getTransactionKindName(txdetails);
    const sender = getTransactionSender(txdata);
    const recipient =
        getTransferObjectTransaction(txdetails) ||
        getTransferSuiTransaction(txdetails);
    const txKindData = formatByTransactionKind(txKindName, txdetails, sender);
    const TabName = `${txKindName} Details`;

    const txHeaderData = {
        txId: txdata.txId,
        status: txdata.status,
        txKindName: txKindName,
        ...(txdata.txError ? { error: txdata.txError } : {}),
    };

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

    const validatorSignatureData = {
        title: 'Validator Signatures',
        content: txdata.authSignInfo.signatures.map((validatorSign) => ({
            value: validatorSign,
            monotypeClass: true,
        })),
    };

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
    const defaultActiveTab = 0;

    const modules =
        txKindData?.module?.value && Array.isArray(txKindData?.module?.value)
            ? {
                  title: 'Modules',
                  content: txKindData?.module?.value,
              }
            : false;

    return (
        <div className={cl(styles.txdetailsbg)}>
            <TxResultHeader data={txHeaderData} />
            <Tabs selected={defaultActiveTab}>
                <section title={TabName} className={styles.txtabs}>
                    <div className={styles.txgridcomponent} id={txdata.txId}>
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
                        {sender && (
                            <section
                                className={cl([
                                    styles.txcomponent,
                                    styles.txsender,
                                ])}
                            >
                                <div className={styles.txaddress}>
                                    <SendReceiveView data={sendreceive} />
                                </div>
                            </section>
                        )}
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
                                <TxModulesWrapper data={modules} />
                            </section>
                        )}
                    </div>
                    <div className={styles.txgridcomponent}>
                        <ItemView data={GasStorageFees} />
                    </div>
                </section>
                <section title="Signatures">
                    <div className={styles.txgridcomponent}>
                        <ItemView data={transactionSignatureData} />
                        <ItemView data={validatorSignatureData} />
                    </div>
                </section>
            </Tabs>
        </div>
    );
}

export default TransactionView;

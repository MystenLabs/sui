// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getMoveCallTransaction,
    getPublishTransaction,
    getTransactionKindName,
    getTransactions,
    getTransactionSender,
    getTransferCoinTransaction,
    getMovePackageContent,
    getObjectId,
} from '@mysten/sui.js';
import cl from 'classnames';

import Longtext from '../../components/longtext/Longtext';
import codestyle from '../../styles/bytecode.module.css';
import { convertNumberToDate } from '../../utils/timeUtils';
import { type DataType } from './TransactionResultType';

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
    timestamp_ms: number;
    gasFee: number;
    txError: string;
    mutated: SuiObjectRef[];
    created: SuiObjectRef[];
};

// Generate an Arr of Obj with Label and Value
// TODO rewrite to use sui.js, verify tx types and dynamically generate list
function formatTxResponse(tx: TxDataProps, txId: string) {
    // TODO: handle multiple transactions
    const txData = getTransactions(tx)[0];
    const txKindName = getTransactionKindName(txData);

    return [
        {
            label: 'Transaction ID',
            value: txId,
            className: 'columnheader',
        },
        {
            label: 'Timestamp',
            value: tx.timestamp_ms,
        },

        {
            // May change later
            label: 'Status',
            value:
                tx.status === 'success' ? 'Success' : `Failed - ${tx.txError}`,
            classAttr:
                tx.status === 'success' ? 'status-success' : 'status-fail',
        },
        {
            label: 'Transaction Type',
            value: txKindName,
        },
        // txKind Transfer or Call
        ...(formatByTransactionKind(
            txKindName,
            txData,
            getTransactionSender(tx)
        ) ?? []),
        {
            label: 'Transaction Signature',
            value: tx.txSignature,
        },
        ...(tx.mutated?.length
            ? [
                  {
                      label: 'Mutated',
                      category: 'objects',
                      value: tx.mutated.map((obj) => getObjectId(obj)),
                      list: true,
                      link: true,
                  },
              ]
            : []),
        ...(tx.created?.length
            ? [
                  {
                      label: 'Created',
                      value: tx.created.map((obj) => getObjectId(obj)),
                      category: 'objects',
                      list: true,
                      link: true,
                  },
              ]
            : []),
        {
            label: 'Gas Payment',
            value: tx.data.gasPayment.objectId,
            link: true,
        },
        {
            label: 'Gas Fee',
            value: tx.gasFee,
        },
        {
            label: 'Gas Budget',
            value: tx.data.gasBudget,
        },
        {
            label: 'Validator Signatures',
            value: tx.authSignInfo.signatures,
            list: true,
            sublist: true,
            // Todo - assumes only two itmes in list ['A', 'B']
            subLabel: ['Name', 'Signature'],
        },
    ];
}

function formatByTransactionKind(
    kind: TransactionKindName | undefined,
    data: SuiTransactionKind,
    sender: string
) {
    switch (kind) {
        case 'TransferCoin':
            const transfer = getTransferCoinTransaction(data)!;
            return [
                {
                    label: 'Object',
                    value: transfer.objectRef.objectId,
                    link: true,
                    category: 'objects',
                },
                {
                    label: 'Sender',
                    value: sender,
                    link: true,
                    category: 'addresses',
                    className: 'Receiver',
                },
                {
                    label: 'To',
                    value: transfer.recipient,
                    category: 'addresses',
                    link: true,
                },
            ];
        case 'Call':
            const moveCall = getMoveCallTransaction(data)!;
            return [
                {
                    label: 'From',
                    value: sender,
                    link: true,
                    category: 'addresses',
                },
                {
                    label: 'Package',
                    category: 'objects',
                    value: getObjectId(moveCall.package),
                    link: true,
                },
                {
                    label: 'Module',
                    value: moveCall.module,
                },
                {
                    label: 'Function',
                    value: moveCall.function,
                },
                {
                    label: 'Arguments',
                    value: JSON.stringify(moveCall.arguments),
                },
            ];
        case 'Publish':
            const publish = getPublishTransaction(data)!;
            return [
                {
                    label: 'Modules',
                    // TODO: render modules correctly
                    value: Object.entries(getMovePackageContent(publish)!),
                    list: true,
                },
                ...(sender
                    ? [
                          {
                              label: 'Sender ',
                              value: sender,
                              link: true,
                              category: 'addresses',
                          },
                      ]
                    : []),
            ];
        default:
            return [];
    }
}

function ItemView({ itm, text }: { itm: any; text: string | number }) {
    switch (true) {
        case itm.label === 'Modules':
            return <div className={codestyle.code}>{itm.value}</div>;
        case itm.label === 'Timestamp':
            return <>{convertNumberToDate(text as number)}</>;
        case itm.link:
            return (
                <Longtext
                    text={text as string}
                    category={itm.category ? itm.category : 'unknown'}
                    isLink={true}
                />
            );
        default:
            return <>{text}</>;
    }
}

function SubListView({ itm, list }: { itm: any; list: any }) {
    return (
        <div>
            {list.map((sublist: string, l: number) => (
                <div className={styles.sublist} key={l}>
                    <div className={styles.sublist}>
                        {itm.subLabel ? (
                            <div className={styles.sublistlabel}>
                                {itm.subLabel[l]}:
                            </div>
                        ) : (
                            ''
                        )}
                        <div className={styles.sublistvalue}>
                            <ItemView itm={itm} text={sublist} />
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}

const TestIDMatcher = (label: string) => {
    switch (label) {
        case 'Transaction ID':
            return 'transactionID';
        case 'Timestamp':
            return 'timestamp';
        default:
            return '';
    }
};

function TransactionView({ txdata }: { txdata: DataType }) {
    return (
        <>
            {txdata && (
                <div>
                    <div id="txview" className={styles.txcard}>
                        {formatTxResponse(txdata, txdata.txId).map(
                            (itm: any, index: number) => (
                                <div
                                    key={index}
                                    className={cl(
                                        styles.txcardgrid,
                                        itm.className
                                            ? styles[itm.className]
                                            : ''
                                    )}
                                >
                                    <div>{itm.label}</div>
                                    <div
                                        className={cl(
                                            styles.txcardgridlarge,
                                            itm.classAttr
                                                ? styles[itm.classAttr]
                                                : ''
                                        )}
                                        id={TestIDMatcher(itm.label)}
                                    >
                                        {itm.list ? (
                                            <ul className={styles.listitems}>
                                                {itm.value.map(
                                                    (list: any, n: number) => (
                                                        <li
                                                            className={
                                                                styles.list
                                                            }
                                                            key={n}
                                                        >
                                                            {itm.sublist ? (
                                                                <SubListView
                                                                    itm={itm}
                                                                    list={list}
                                                                />
                                                            ) : (
                                                                <ItemView
                                                                    itm={itm}
                                                                    text={list}
                                                                />
                                                            )}
                                                        </li>
                                                    )
                                                )}
                                            </ul>
                                        ) : (
                                            <ItemView
                                                itm={itm}
                                                text={itm.value}
                                            />
                                        )}
                                    </div>
                                </div>
                            )
                        )}
                    </div>
                </div>
            )}
        </>
    );
}

export default TransactionView;

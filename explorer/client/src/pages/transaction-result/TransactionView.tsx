// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getMoveCallTransaction,
    getPublishTransaction,
    getTransactionKind,
    getTransferTransaction,
} from '@mysten/sui.js';
import { Buffer } from 'buffer';
import cl from 'classnames';

import Longtext from '../../components/longtext/Longtext';
import { type DataType } from './TransactionResultType';

import type {
    CertifiedTransaction,
    TransactionData,
    TransactionKindName,
    ExecutionStatusType,
    RawObjectRef,
} from '@mysten/sui.js';

import styles from './TransactionView.module.css';

type TxDataProps = CertifiedTransaction & {
    status: ExecutionStatusType;
    gasFee: number;
    txError: string;
    mutated: RawObjectRef[];
    created: RawObjectRef[];
};

// Generate an Arr of Obj with Label and Value
// TODO rewrite to use sui.js, verify tx types and dynamically generate list
function formatTxResponse(tx: TxDataProps, txId: string) {
    // Todo add batch kind
    const txKindName = getTransactionKind(tx.data);

    return [
        {
            label: 'Transaction ID',
            value: txId,
            className: 'columnheader',
        },
        {
            // May change later
            label: 'Status',
            value:
                tx.status === 'Success' ? 'Success' : `Failed - ${tx.txError}`,
            classAttr:
                tx.status === 'Success' ? 'status-success' : 'status-fail',
        },
        {
            label: 'Transaction Type',
            value: txKindName,
        },
        // txKind Transfer or Call
        ...(formatByTransactionKind(txKindName, tx.data) ?? []),
        {
            label: 'Transaction Signature',
            value: tx.tx_signature,
        },
        ...(tx.mutated.length
            ? [
                  {
                      label: 'Mutated',
                      category: 'objects',
                      value: tx.mutated.map((obj) => obj[0]),
                      list: true,
                      link: true,
                  },
              ]
            : []),
        ...(tx.created.length
            ? [
                  {
                      label: 'Created',
                      value: tx.created.map((obj) => obj[0]),
                      category: 'objects',
                      list: true,
                      link: true,
                  },
              ]
            : []),
        {
            label: 'Gas Payment',
            value: tx.data.gas_payment[0],
            link: true,
        },
        {
            label: 'Gas Fee',
            value: tx.gasFee,
        },
        {
            label: 'Gas Budget',
            value: tx.data.gas_budget,
        },
        {
            label: 'Validator Signatures',
            value: tx.auth_sign_info.signatures,
            list: true,
            sublist: true,
            // Todo - assumes only two itmes in list ['A', 'B']
            subLabel: ['Name', 'Signature'],
        },
    ];
}

function formatByTransactionKind(
    kind: TransactionKindName | undefined,
    data: TransactionData
) {
    switch (kind) {
        case 'Transfer':
            const transfer = getTransferTransaction(data)!;
            return [
                {
                    label: 'Object',
                    value: transfer.object_ref[0],
                    link: true,
                    category: 'objects',
                },
                {
                    label: 'Sender',
                    value: data.sender,
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
                    value: data.sender,
                    link: true,
                    category: 'addresses',
                },
                {
                    label: 'Package',
                    category: 'objects',
                    value: moveCall.package[0],
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
                    // convert pure type
                    value: moveCall.arguments
                        .filter((itm: any) => itm['Pure'])
                        .map((data: any) =>
                            Buffer.from(data['Pure']).toString('base64')
                        ),
                },
            ];
        case 'Publish':
            const publish = getPublishTransaction(data)!;
            return [
                {
                    label: 'Modules',
                    value: publish.modules,
                    list: true,
                },
                ...(data.sender
                    ? [
                          {
                              label: 'Sender ',
                              value: data.sender,
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

function TransactionView({ txdata }: { txdata: DataType }) {
    return (
        <>
            {txdata && (
                <div className={styles.transactioncard}>
                    <div className={styles.txcard}>
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
                                        id={
                                            itm.label === 'Transaction ID'
                                                ? 'transactionID'
                                                : ''
                                        }
                                    >
                                        {itm.list ? (
                                            <ul className={styles.listitems}>
                                                {itm.value.map(
                                                    (list: any, n: number) =>
                                                        itm.sublist ? (
                                                            <li
                                                                className={
                                                                    styles.list
                                                                }
                                                                key={n}
                                                            >
                                                                <div>
                                                                    {list.map(
                                                                        (
                                                                            sublist: string,
                                                                            l: number
                                                                        ) => (
                                                                            <div
                                                                                className={
                                                                                    styles.sublist
                                                                                }
                                                                                key={
                                                                                    l
                                                                                }
                                                                            >
                                                                                <div
                                                                                    className={
                                                                                        styles.sublist
                                                                                    }
                                                                                >
                                                                                    {itm.subLabel ? (
                                                                                        <div
                                                                                            className={
                                                                                                styles.sublistlabel
                                                                                            }
                                                                                        >
                                                                                            {
                                                                                                itm
                                                                                                    .subLabel[
                                                                                                    l
                                                                                                ]
                                                                                            }

                                                                                            :
                                                                                        </div>
                                                                                    ) : (
                                                                                        ''
                                                                                    )}
                                                                                    <div
                                                                                        className={
                                                                                            styles.sublistvalue
                                                                                        }
                                                                                    >
                                                                                        {itm.link ? (
                                                                                            <Longtext
                                                                                                text={
                                                                                                    sublist
                                                                                                }
                                                                                                category={
                                                                                                    itm.category
                                                                                                        ? itm.category
                                                                                                        : 'unknown'
                                                                                                }
                                                                                                isLink={
                                                                                                    true
                                                                                                }
                                                                                            />
                                                                                        ) : (
                                                                                            sublist
                                                                                        )}
                                                                                    </div>
                                                                                </div>
                                                                            </div>
                                                                        )
                                                                    )}
                                                                </div>
                                                            </li>
                                                        ) : (
                                                            <li
                                                                className={
                                                                    styles.list
                                                                }
                                                                key={n}
                                                            >
                                                                {itm.link ? (
                                                                    <Longtext
                                                                        text={
                                                                            list
                                                                        }
                                                                        category={
                                                                            itm.category
                                                                                ? itm.category
                                                                                : 'unknown'
                                                                        }
                                                                        isLink={
                                                                            true
                                                                        }
                                                                    />
                                                                ) : (
                                                                    list
                                                                )}
                                                            </li>
                                                        )
                                                )}
                                            </ul>
                                        ) : itm.link ? (
                                            <Longtext
                                                text={itm.value}
                                                category={
                                                    itm.category
                                                        ? itm.category
                                                        : 'unknown'
                                                }
                                                isLink={true}
                                            />
                                        ) : (
                                            itm.value
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

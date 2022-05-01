// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Buffer } from 'buffer';
import cl from 'classnames';
import {
    getMoveCallTransaction,
    getPublishTransaction,
    getTransactionKind,
    getTransferTransaction,
} from 'sui.js';

import Longtext from '../../components/longtext/Longtext';

import type {
    CertifiedTransaction,
    TransactionData,
    TransactionKindName,
} from 'sui.js';

import styles from './TransactionCard.module.css';

// Generate an Arr of Obj with Label and Value
// TODO rewrite to use sue.js, verify tx types and dynamically generate list
function formatTxResponse(tx: CertifiedTransaction, txId: string) {
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
            value: 'Success',
            classAttr: 'status-success',
        },
        {
            label: 'Transaction Type',
            value: txKindName,
        },
        // txKind Transfer or Call
        ...(formatByTransactionKind(txKindName, tx.data) ?? []),
        {
            label: 'Transactions Signature',
            value: tx.tx_signature,
        },

        {
            label: 'Gas Payment',
            value: tx.data.gas_payment[0],
            link: true,
            className: 'grouped',
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
                    value: moveCall.package,
                    list: true,
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
                    // list: true,
                },
            ];
        case 'Publish':
            const publish = getPublishTransaction(data)!;
            return [
                {
                    label: 'Modules',
                    value: publish.modules,
                    list: true,
                    //  sublist: true,
                },
            ];
        default:
            return [];
    }
}

type Props = {
    txdata: CertifiedTransaction & { loadState: string; txId: string };
};

function TransactionCard({ txdata }: Props) {
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
                                    >
                                        {itm.list ? (
                                            <ul className={styles.listitems}>
                                                {itm.value.map(
                                                    (list: any, n: number) =>
                                                        itm.sublist ? (
                                                            <ul
                                                                className={
                                                                    styles.list
                                                                }
                                                                key={n}
                                                            >
                                                                {list.map(
                                                                    (
                                                                        sublist: string,
                                                                        l: number
                                                                    ) => (
                                                                        <li
                                                                            className={
                                                                                styles.sublist
                                                                            }
                                                                            key={
                                                                                l
                                                                            }
                                                                        >
                                                                            {
                                                                                sublist
                                                                            }
                                                                        </li>
                                                                    )
                                                                )}
                                                            </ul>
                                                        ) : (
                                                            <li
                                                                className={
                                                                    styles.list
                                                                }
                                                                key={n}
                                                            >
                                                                {list}
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

export default TransactionCard;

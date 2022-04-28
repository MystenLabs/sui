// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Buffer } from 'buffer';
import cl from 'classnames';

import Longtext from '../../components/longtext/Longtext';

import styles from './TransactionCard.module.css';

// Generate an Arr of Obj with Label and Value
// TODO rewrite to use sue.js, verify tx types and dynamically generate list
function formatTxResponse(data: any) {
    const tx = data.transaction;

    // Todo add batch kind
    const txKind = tx.data.kind;
    const txKindName = Object.keys(txKind.Single)[0];
    return [
        {
            label: 'Transaction ID',
            value: data.txId,
        },
        {
            label: 'Result/Status',
            value: 'Success',
            classAttr: 'status-success',
        },
        {
            label: 'Transaction Kind',
            value: txKindName,
        },
        // txKind Transfer or Call
        ...(txKindName === 'Transfer'
            ? [
                  {
                      label: 'Object Reference',
                      value: txKind.Single.Transfer.object_ref,
                      list: true,
                  },
                  {
                      label: 'From',
                      value: tx.data.sender,
                      link: true,
                      group: 'grouped',
                  },
                  {
                      label: 'To',
                      value: txKind.Single.Transfer.recipient,
                      link: true,
                  },
              ]
            : [
                  {
                      label: 'From',
                      value: tx.data.sender,
                      link: true,
                  },
                  {
                      label: 'Package',
                      value: txKind.Single.Call.package,
                      list: true,
                  },
                  {
                      label: 'Module',
                      value: txKind.Single.Call.module,
                  },
                  {
                      label: 'Function',
                      value: txKind.Single.Call.function,
                  },
                  {
                      label: 'Arguments',
                      value: txKind.Single.Call.arguments.map((data: any) =>
                          Buffer.from(data['Pure']).toString('base64')
                      ),
                      list: true,
                  },
              ]),

        {
            label: 'Tx Signature',
            value: tx.tx_signature,
        },

        {
            label: 'Gas Payment',
            value: tx.data.gas_payment,
            link: true,
            list: true,
        },
        {
            label: 'Gas Budget',
            value: tx.data.gas_budget,
        },
        {
            label: 'Signatures',
            value: data.signatures,
            list: true,
            sublist: true,
        },
    ];
}

function TransactionCard({ txdata }: any) {
    return (
        <>
            {txdata?.transaction && (
                <div className={styles.transactioncard}>
                    <div className={styles.txcard}>
                        {formatTxResponse(txdata).map(
                            (itm: any, index: number) => (
                                <div
                                    key={index}
                                    className={cl(
                                        styles.txcardgrid,
                                        itm.group ? styles[itm.group] : ''
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
                                                category="unknown"
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

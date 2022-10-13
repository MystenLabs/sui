// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTotalGasUsed,
    getTransactions,
    getTransactionDigest,
    getTransactionKindName,
    getTransferObjectTransaction,
    getTransferSuiTransaction,
    getTransferSuiAmount,
    type GetTxnDigestsResponse,
    type CertifiedTransaction,
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import { Fragment } from 'react';

import { ReactComponent as ContentSuccessStatus } from '../../assets/SVGIcons/12px/Check.svg';
import { ReactComponent as ContentFailedStatus } from '../../assets/SVGIcons/12px/X.svg';
import { ReactComponent as ContentArrowRight } from '../../assets/SVGIcons/16px/ArrowRight.svg';
import Longtext from '../../components/longtext/Longtext';
import { DefaultRpcClient } from '../../utils/api/DefaultRpcClient';
import { type Network } from '../../utils/api/rpcSetting';
import { numberSuffix } from '../../utils/numberUtil';
import { deduplicate } from '../../utils/searchUtil';
import { truncate, presentBN } from '../../utils/stringUtils';
import { timeAgo } from '../../utils/timeUtils';

import styles from './RecentTxCard.module.css';

export type TxnData = {
    To?: string;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    suiAmount: bigint;
    kind: TransactionKindName | undefined;
    From: string;
    timestamp_ms?: number;
};

export type LinkObj = {
    url: string;
    name?: string;
    copy?: boolean;
    category?: string;
    isLink?: boolean;
};

type Category =
    | 'objects'
    | 'transactions'
    | 'addresses'
    | 'ethAddress'
    | 'unknown';

type TxStatus = {
    txTypeName: TransactionKindName | undefined;
    status: ExecutionStatusType;
};

export function SuiAmount({ amount }: { amount: bigint | string | undefined }) {
    if (amount) {
        const SuiSuffix = <abbr className={styles.suisuffix}>SUI</abbr>;

        if (typeof amount === 'bigint') {
            return (
                <section>
                    <span>
                        {presentBN(amount)}
                        {SuiSuffix}
                    </span>
                </section>
            );
        }
        if (typeof amount === 'string') {
            return (
                <section>
                    <span className={styles.suiamount}>
                        {amount}
                        {SuiSuffix}
                    </span>
                </section>
            );
        }
    }

    return <span className={styles.suiamount}>--</span>;
}

export function TxAddresses({ content }: { content: LinkObj[] }) {
    return (
        <section className={styles.addresses}>
            {content.map((itm, idx) => (
                <Fragment key={idx + itm.url}>
                    <Longtext
                        text={itm.url}
                        alttext={itm.name}
                        category={(itm.category as Category) || 'unknown'}
                        isLink={itm?.isLink}
                        copyButton={itm?.copy ? '16' : 'none'}
                    />
                    {idx !== content.length - 1 && <ContentArrowRight />}
                </Fragment>
            ))}
        </section>
    );
}

function TxStatusType({ content }: { content: TxStatus }) {
    const TxStatus = {
        success: ContentSuccessStatus,
        fail: ContentFailedStatus,
    };
    const TxResultStatus =
        content.status === 'success' ? TxStatus.success : TxStatus.fail;
    return (
        <section className={styles.statuswrapper}>
            <div
                className={
                    content.status === 'success' ? styles.success : styles.fail
                }
            >
                <TxResultStatus /> <div>{content.txTypeName}</div>
            </div>
        </section>
    );
}

function TxTimeType({ timestamp }: { timestamp: number | undefined }) {
    return (
        <section>
            <div
                style={{
                    width: '85px',
                }}
            >{`${timeAgo(timestamp, undefined, true)}`}</div>
        </section>
    );
}

// Generate table data from the transaction data
export const genTableDataFromTxData = (
    results: TxnData[],
    truncateLength: number
) => {
    return {
        data: results.map((txn) => ({
            date: <TxTimeType timestamp={txn.timestamp_ms} />,
            transactionId: (
                <TxAddresses
                    content={[
                        {
                            url: txn.txId,
                            name: truncate(txn.txId, truncateLength),
                            category: 'transactions',
                            isLink: true,
                            copy: false,
                        },
                    ]}
                />
            ),
            addresses: (
                <TxAddresses
                    content={[
                        {
                            url: txn.From,
                            name: truncate(txn.From, truncateLength),
                            category: 'addresses',
                            isLink: true,
                            copy: false,
                        },
                        ...(txn.To
                            ? [
                                  {
                                      url: txn.To,
                                      name: truncate(txn.To, truncateLength),
                                      category: 'addresses',
                                      isLink: true,
                                      copy: false,
                                  },
                              ]
                            : []),
                    ]}
                />
            ),
            txTypes: (
                <TxStatusType
                    content={{
                        txTypeName: txn.kind,
                        status: txn.status,
                    }}
                />
            ),
            amounts: <SuiAmount amount={txn.suiAmount} />,
            gas: <SuiAmount amount={numberSuffix(txn.txGas)} />,
        })),
        columns: [
            {
                headerLabel: 'Time',
                accessorKey: 'date',
            },
            {
                headerLabel: 'Type',
                accessorKey: 'txTypes',
            },
            {
                headerLabel: () => (
                    <div className={styles.addresses}>Transaction ID</div>
                ),
                accessorKey: 'transactionId',
            },
            {
                headerLabel: () => (
                    <div className={styles.addresses}>Addresses</div>
                ),
                accessorKey: 'addresses',
            },
            {
                headerLabel: 'Amount',
                accessorKey: 'amounts',
            },
            {
                headerLabel: 'Gas',
                accessorKey: 'gas',
            },
        ],
    };
};

export const getDataOnTxDigests = (
    network: Network | string,
    transactions: GetTxnDigestsResponse
) =>
    DefaultRpcClient(network)
        .getTransactionWithEffectsBatch(deduplicate(transactions))
        .then((txEffs) => {
            return (
                txEffs
                    .map((txEff) => {
                        const digest = transactions.filter(
                            (transactionId) =>
                                transactionId ===
                                getTransactionDigest(txEff.certificate)
                        )[0];
                        const res: CertifiedTransaction = txEff.certificate;
                        // TODO: handle multiple transactions
                        const txns = getTransactions(res);
                        if (txns.length > 1) {
                            console.error(
                                'Handling multiple transactions is not yet supported',
                                txEff
                            );
                            return null;
                        }
                        const txn = txns[0];
                        const txKind = getTransactionKindName(txn);
                        const recipient =
                            getTransferObjectTransaction(txn)?.recipient ||
                            getTransferSuiTransaction(txn)?.recipient;

                        return {
                            txId: digest,
                            status: getExecutionStatusType(txEff)!,
                            txGas: getTotalGasUsed(txEff),
                            suiAmount: getTransferSuiAmount(txn),
                            kind: txKind,
                            From: res.data.sender,
                            timestamp_ms: txEff.timestamp_ms,
                            ...(recipient
                                ? {
                                      To: recipient,
                                  }
                                : {}),
                        };
                    })
                    // Remove failed transactions and sort by sequence number
                    .filter((itm) => itm)
            );
        });

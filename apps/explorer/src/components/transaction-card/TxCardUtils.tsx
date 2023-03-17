// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { X12 } from '@mysten/icons';
import {
    type ExecutionStatusType,
    getExecutionStatusType,
    getTotalGasUsed,
    getTransactionDigest,
    getTransactionKind,
    getTransactionKindName,
    getTransactionSender,
    type GetTxnDigestsResponse,
    type JsonRpcProvider,
    SUI_TYPE_ARG,
    type TransactionKindName,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import { getAmount } from '../../utils/getAmount';
import { TxTimeType } from '../tx-time/TxTimeType';

import styles from './RecentTxCard.module.css';

import { AddressLink, TransactionLink } from '~/ui/InternalLink';
import { TransactionType } from '~/ui/TransactionType';

export type TxnData = {
    To?: string;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    suiAmount: bigint | number;
    coinType?: string | null;
    kind: TransactionKindName | undefined;
    From: string;
    timestamp_ms?: number;
};

export function SuiAmount({
    amount,
}: {
    amount: bigint | number | string | undefined | null;
}) {
    const [formattedAmount, coinType] = useFormatCoin(amount, SUI_TYPE_ARG);

    if (amount) {
        const SuiSuffix = <abbr className={styles.suisuffix}>{coinType}</abbr>;

        return (
            <section>
                <span className={styles.suiamount}>
                    {formattedAmount}
                    {SuiSuffix}
                </span>
            </section>
        );
    }

    return <span className={styles.suiamount}>--</span>;
}

function TxTableHeader({ label }: { label: string }) {
    return <div className="pl-3">{label}</div>;
}

function TxTableCol({
    isHighlightedOnHover,
    children,
}: {
    isHighlightedOnHover?: boolean;
    children: ReactNode;
}) {
    return (
        <div
            className={clsx(
                'flex h-full items-center rounded px-3',
                isHighlightedOnHover && 'hover:bg-sui-light'
            )}
        >
            {children}
        </div>
    );
}

// Generate table data from the transaction data
export const genTableDataFromTxData = (results: TxnData[]) => ({
    data: results.map((txn) => ({
        date: <TxTimeType timestamp={txn.timestamp_ms} />,
        transactionId: (
            <TxTableCol isHighlightedOnHover>
                <TransactionLink
                    digest={txn.txId}
                    before={
                        txn.status === 'success' ? (
                            <div className="h-2 w-2 rounded-full bg-success" />
                        ) : (
                            <X12 className="text-issue-dark" />
                        )
                    }
                />
            </TxTableCol>
        ),
        txTypes: (
            <TransactionType
                isSuccess={txn.status === 'success'}
                type={txn.kind}
            />
        ),
        amounts: (
            <TxTableCol>
                <SuiAmount amount={txn.suiAmount} />
            </TxTableCol>
        ),
        gas: (
            <TxTableCol>
                <SuiAmount amount={txn.txGas} />
            </TxTableCol>
        ),
        sender: (
            <TxTableCol isHighlightedOnHover>
                <AddressLink address={txn.From} />
            </TxTableCol>
        ),
    })),
    columns: [
        {
            header: () => <TxTableHeader label="Transaction ID" />,
            accessorKey: 'transactionId',
        },
        {
            header: () => <TxTableHeader label="Sender" />,
            accessorKey: 'sender',
        },
        {
            header: () => <TxTableHeader label="Amount" />,
            accessorKey: 'amounts',
        },
        {
            header: () => <TxTableHeader label="Gas" />,
            accessorKey: 'gas',
        },
        {
            header: () => <TxTableHeader label="Time" />,
            accessorKey: 'date',
        },
    ],
});

const dedupe = (arr: string[]) => Array.from(new Set(arr));

export const getDataOnTxDigests = (
    rpc: JsonRpcProvider,
    transactions: GetTxnDigestsResponse
) =>
    rpc
        .multiGetTransactions({
            digests: dedupe(transactions),
            options: {
                showInput: true,
                showEffects: true,
                showEvents: true,
            },
        })
        .then((txEffs) =>
            txEffs
                .map((txEff) => {
                    const digest = transactions.filter(
                        (transactionId) =>
                            transactionId === getTransactionDigest(txEff)
                    )[0];
                    const txn = getTransactionKind(txEff)!;
                    const txKind = getTransactionKindName(txn);
                    const recipient = null;
                    //     getTransferObjectTransaction(txn)?.recipient ||
                    //     getTransferSuiTransaction(txn)?.recipient;

                    const transfer = getAmount({
                        txnData: txEff,
                        suiCoinOnly: true,
                    })[0];

                    // use only absolute value of sui amount
                    const suiAmount = transfer?.amount
                        ? Math.abs(transfer.amount)
                        : null;

                    return {
                        txId: digest,
                        status: getExecutionStatusType(txEff)!,
                        txGas: getTotalGasUsed(txEff),
                        suiAmount,
                        coinType: transfer?.coinType || null,
                        kind: txKind,
                        From: getTransactionSender(txEff),
                        timestamp_ms: txEff.timestampMs,
                        ...(recipient
                            ? {
                                  To: recipient,
                              }
                            : {}),
                    };
                })
                // Remove failed transactions
                .filter((itm) => itm)
        );

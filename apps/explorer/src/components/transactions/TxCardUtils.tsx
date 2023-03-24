// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { X12 } from '@mysten/icons';
import {
    getExecutionStatusType,
    getTotalGasUsed,
    getTransactionSender,
    type JsonRpcProvider,
    SUI_TYPE_ARG,
    type SuiTransactionResponse,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import { getAmount } from '../../utils/getAmount';
import { TxTimeType } from '../tx-time/TxTimeType';

import styles from './RecentTxCard.module.css';

import { AddressLink, TransactionLink } from '~/ui/InternalLink';

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

export function TxTableCol({
    isHighlightedOnHover,
    isFirstCol,
    children,
}: {
    isHighlightedOnHover?: boolean;
    isFirstCol?: boolean;
    children: ReactNode;
}) {
    return (
        <div
            className={clsx(
                'flex h-full items-center rounded',
                !isFirstCol && 'px-3',
                isHighlightedOnHover && 'hover:bg-sui-light'
            )}
        >
            {children}
        </div>
    );
}

// Generate table data from the transaction data
export const genTableDataFromTxData = (results: SuiTransactionResponse[]) => ({
    data: results.map((transaction) => {
        const status = getExecutionStatusType(transaction);
        const transfer = getAmount({
            txnData: transaction,
            suiCoinOnly: true,
        })[0];

        // use only absolute value of sui amount
        const suiAmount = transfer?.amount ? Math.abs(transfer.amount) : null;
        const sender = getTransactionSender(transaction);

        return {
            date: (
                <TxTableCol>
                    <TxTimeType timestamp={transaction.timestampMs} />
                </TxTableCol>
            ),
            transactionId: (
                <TxTableCol isFirstCol isHighlightedOnHover>
                    <TransactionLink
                        digest={transaction.digest}
                        before={
                            status === 'success' ? (
                                <div className="h-2 w-2 rounded-full bg-success" />
                            ) : (
                                <X12 className="text-issue-dark" />
                            )
                        }
                    />
                </TxTableCol>
            ),
            amounts: (
                <TxTableCol>
                    <SuiAmount amount={suiAmount} />
                </TxTableCol>
            ),
            gas: (
                <TxTableCol>
                    <SuiAmount amount={getTotalGasUsed(transaction)} />
                </TxTableCol>
            ),
            sender: (
                <TxTableCol isHighlightedOnHover>
                    {sender ? <AddressLink address={sender} /> : '-'}
                </TxTableCol>
            ),
        };
    }),
    columns: [
        {
            header: 'Transaction ID',
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
    transactions: string[]
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
        .then((transactions) =>
            // Remove failed transactions
            transactions.filter((item) => item)
        );

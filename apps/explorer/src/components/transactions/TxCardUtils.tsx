// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { X12, Dot12 } from '@mysten/icons';
import {
    getExecutionStatusType,
    getTotalGasUsed,
    getTransactionSender,
    type JsonRpcProvider,
    SUI_TYPE_ARG,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { type ReactNode } from 'react';

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
            <span className={styles.suiamount}>
                {formattedAmount}
                {SuiSuffix}
            </span>
        );
    }

    return <span className={styles.suiamount}>--</span>;
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
                'flex h-full min-w-[50px] items-center rounded',
                !isFirstCol && 'pr-3',
                isHighlightedOnHover && 'hover:bg-sui-light'
            )}
        >
            {children}
        </div>
    );
}

// Generate table data from the transaction data
export const genTableDataFromTxData = (
    results: SuiTransactionBlockResponse[]
) => ({
    data: results.map((transaction) => {
        const status = getExecutionStatusType(transaction);
        const sender = getTransactionSender(transaction);

        return {
            time: (
                <TxTableCol>
                    <TxTimeType timestamp={transaction.timestampMs} />
                </TxTableCol>
            ),
            digest: (
                <TxTableCol isFirstCol isHighlightedOnHover>
                    <TransactionLink
                        digest={transaction.digest}
                        before={
                            status === 'success' ? (
                                <Dot12 className="text-success" />
                            ) : (
                                <X12 className="text-issue-dark" />
                            )
                        }
                    />
                </TxTableCol>
            ),
            txns: (
                <TxTableCol>
                    {transaction.transaction?.data.transaction.kind ===
                    'ProgrammableTransaction'
                        ? transaction.transaction.data.transaction.transactions
                              .length
                        : '--'}
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
            header: 'Digest',
            accessorKey: 'digest',
        },
        {
            header: 'Sender',
            accessorKey: 'sender',
        },
        {
            header: 'Txns',
            accessorKey: 'txns',
        },
        {
            header: 'Gas',
            accessorKey: 'gas',
        },
        {
            header: 'Time',
            accessorKey: 'time',
        },
    ],
});

const dedupe = (arr: string[]) => Array.from(new Set(arr));

export const getDataOnTxDigests = (
    rpc: JsonRpcProvider,
    transactions: string[]
) =>
    rpc
        .multiGetTransactionBlocks({
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

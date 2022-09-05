// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import BN from 'bn.js';

import { numberSuffix } from '../../utils/numberUtil';
import { truncate, presentBN } from '../../utils/stringUtils';
import { timeAgo } from '../../utils/timeUtils';

import styles from './RecentTxCard.module.css';

export type TxnData = {
    To?: string;
    seq: number;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    suiAmount: BN;
    kind: TransactionKindName | undefined;
    From: string;
    timestamp_ms?: number;
};

export function SuiAmount({ amount }: { amount: BN | string | undefined }) {
    if (amount) {
        const SuiSuffix = <abbr className={styles.suisuffix}>SUI</abbr>;

        if (BN.isBN(amount)) {
            return (
                <span>
                    {presentBN(amount)}
                    {SuiSuffix}
                </span>
            );
        }
        if (typeof amount === 'string') {
            return (
                <span className={styles.suiamount}>
                    {amount}
                    {SuiSuffix}
                </span>
            );
        }
    }

    return <span className={styles.suiamount}>--</span>;
}

// Generate table data from the transaction data
export const genTableDataFromTxData = (
    results: TxnData[],
    truncateLength: number
) => {
    return {
        data: results.map((txn) => ({
            date: `${timeAgo(txn.timestamp_ms, undefined, true)} ago`,
            transactionId: [
                {
                    url: txn.txId,
                    name: truncate(txn.txId, truncateLength),
                    category: 'transactions',
                    isLink: true,
                    copy: false,
                },
            ],
            addresses: [
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
            ],
            txTypes: {
                txTypeName: txn.kind,
                status: txn.status,
            },
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
                headerLabel: 'Transaction ID',
                accessorKey: 'transactionId',
            },
            {
                headerLabel: 'Addresses',
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

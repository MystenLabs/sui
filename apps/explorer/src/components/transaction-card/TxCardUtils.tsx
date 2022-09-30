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

export function SuiAmount({ amount }: { amount: bigint | string | undefined }) {
    if (amount) {
        const SuiSuffix = <abbr className={styles.suisuffix}>SUI</abbr>;

        if (typeof amount === 'bigint') {
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
            date: <div
                    style={{
                        width: '85px',
                    }}
                >`${timeAgo(txn.timestamp_ms, undefined, true)}`</div>,
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

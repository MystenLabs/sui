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
    SUI_TYPE_ARG,
    type GetTxnDigestsResponse,
    type CertifiedTransaction,
    type ExecutionStatusType,
    type TransactionKindName,
    type JsonRpcProvider,
} from '@mysten/sui.js';
import { Fragment } from 'react';

import { ReactComponent as ContentSuccessStatus } from '../../assets/SVGIcons/12px/Check.svg';
import { ReactComponent as ContentFailedStatus } from '../../assets/SVGIcons/12px/X.svg';
import { ReactComponent as ContentArrowRight } from '../../assets/SVGIcons/16px/ArrowRight.svg';
import Longtext from '../../components/longtext/Longtext';
import { getAmount } from '../../utils/getAmount';
import { deduplicate } from '../../utils/searchUtil';
import { truncate } from '../../utils/stringUtils';
import { TxTimeType } from '../tx-time/TxTimeType';

import styles from './RecentTxCard.module.css';

import { useFormatCoin } from '~/hooks/useFormatCoin';

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

export function SuiAmount({
    amount,
}: {
    amount: bigint | number | string | undefined | null;
}) {
    const [formattedAmount] = useFormatCoin(amount, SUI_TYPE_ARG);

    if (amount) {
        const SuiSuffix = <abbr className={styles.suisuffix}>SUI</abbr>;

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
            gas: <SuiAmount amount={txn.txGas} />,
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
    rpc: JsonRpcProvider,
    transactions: GetTxnDigestsResponse
) =>
    rpc
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

                        const txnTransfer = getAmount(txn, txEff.effects)?.[0];

                        return {
                            txId: digest,
                            status: getExecutionStatusType(txEff)!,
                            txGas: getTotalGasUsed(txEff),
                            suiAmount: txnTransfer?.amount || null,
                            coinType: txnTransfer?.coinType || null,
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
                    // Remove failed transactions
                    .filter((itm) => itm)
            );
        });

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getSingleTransactionKind,
    getTransactionKind,
    getTransferTransaction,
    getExecutionStatusType,
    getTotalGasUsed,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';

import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import ErrorResult from '../error-result/ErrorResult';

import type {
    CertifiedTransaction,
    GetTxnDigestsResponse,
    TransactionEffectsResponse,
    ExecutionStatusType,
    TransactionKindName,
} from '@mysten/sui.js';

import styles from './RecentTxCard.module.css';

const initState: { loadState: string; latestTx: TxnData[] } = {
    loadState: 'pending',
    latestTx: [],
};

type TxnData = {
    To?: string;
    seq: number;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    kind: TransactionKindName | undefined;
    From: string;
};

async function getRecentTransactions(txNum: number): Promise<TxnData[]> {
    try {
        // Get the latest transactions
        const transactions = await rpc
            .getRecentTransactions(txNum)
            .then((res: GetTxnDigestsResponse) => res);

        const digests = transactions.map((tx) => tx[1]);
        const txLatest = await rpc
            .getTransactionWithEffectsBatch(digests)
            .then((txEffs: TransactionEffectsResponse[]) => {
                return txEffs.map((txEff, i) => {
                    const [seq, digest] = transactions[i];
                    const res: CertifiedTransaction = txEff.certificate;
                    const singleTransaction = getSingleTransactionKind(
                        res.data
                    );
                    if (!singleTransaction) {
                        throw new Error(
                            `Transaction kind not supported yet ${res.data.kind}`
                        );
                    }
                    const txKind = getTransactionKind(res.data);
                    const recipient = getTransferTransaction(
                        res.data
                    )?.recipient;

                    return {
                        seq,
                        txId: digest,
                        status: getExecutionStatusType(txEff.effects.status),
                        txGas: getTotalGasUsed(txEff.effects.status),
                        kind: txKind,
                        From: res.data.sender,
                        ...(recipient
                            ? {
                                  To: recipient,
                              }
                            : {}),
                    };
                });
            });

        // Remove failed transactions and sort by sequence number
        return txLatest
            .filter((itm) => itm)
            .sort((a, b) => b!.seq - a!.seq) as TxnData[];
    } catch (error) {
        throw error;
    }
}

function truncate(fullStr: string, strLen: number, separator: string) {
    if (fullStr.length <= strLen) return fullStr;

    separator = separator || '...';

    const sepLen = separator.length,
        charsToShow = strLen - sepLen,
        frontChars = Math.ceil(charsToShow / 2),
        backChars = Math.floor(charsToShow / 2);

    return (
        fullStr.substr(0, frontChars) +
        separator +
        fullStr.substr(fullStr.length - backChars)
    );
}

function LatestTxCard() {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    useEffect(() => {
        let isMounted = true;
        getRecentTransactions(15)
            .then((resp: any) => {
                if (isMounted) {
                    setIsLoaded(true);
                }
                setResults({
                    loadState: 'loaded',
                    latestTx: resp,
                });
            })
            .catch((err) => {
                setResults({
                    ...initState,
                    loadState: 'fail',
                });
                setIsLoaded(false);
            });

        return () => {
            isMounted = false;
        };
    }, []);
    if (results.loadState === 'pending') {
        return (
            <div className={theme.textresults}>
                <div className={styles.content}>Loading...</div>
            </div>
        );
    }

    if (!isLoaded && results.loadState === 'fail') {
        return (
            <ErrorResult
                id=""
                errorMsg="There was an issue getting the latest transactions"
            />
        );
    }

    if (results.loadState === 'loaded' && !results.latestTx.length) {
        return <ErrorResult id="" errorMsg="No Transactions Found" />;
    }

    return (
        <div className={styles.txlatestesults}>
            <div className={styles.txcardgrid}>
                <h3>Latest Transactions</h3>
            </div>
            <div className={styles.transactioncard}>
                <div>
                    <div
                        className={cl(
                            styles.txcardgrid,
                            styles.txcard,
                            styles.txheader
                        )}
                    >
                        <div className={styles.txcardgridlarge}>TxId</div>
                        <div className={styles.txtype}>TxType</div>
                        <div className={styles.txstatus}>Status</div>
                        <div className={styles.txgas}>Gas</div>
                        <div className={styles.txadd}>Addresses</div>
                    </div>
                    {results.latestTx.map((tx, index) => (
                        <div
                            key={index}
                            className={cl(styles.txcardgrid, styles.txcard)}
                        >
                            <div className={styles.txcardgridlarge}>
                                <div className={styles.txlink}>
                                    <Longtext
                                        text={tx.txId}
                                        category="transactions"
                                        isLink={true}
                                        alttext={truncate(tx.txId, 26, '...')}
                                    />
                                </div>
                            </div>
                            <div className={styles.txtype}> {tx.kind}</div>
                            <div
                                className={cl(
                                    styles[tx.status.toLowerCase()],
                                    styles.txstatus
                                )}
                            >
                                {tx.status === 'Success' ? '✔' : '✖'}
                            </div>
                            <div className={styles.txgas}>{tx.txGas}</div>
                            <div className={styles.txadd}>
                                <div>
                                    From:
                                    <Link
                                        className={styles.txlink}
                                        to={'addresses/' + tx.From}
                                    >
                                        {truncate(tx.From, 25, '...')}
                                    </Link>
                                </div>
                                {tx.To && (
                                    <div>
                                        To :
                                        <Link
                                            className={styles.txlink}
                                            to={'addresses/' + tx.To}
                                        >
                                            {truncate(tx.To, 25, '...')}
                                        </Link>
                                    </div>
                                )}
                            </div>
                        </div>
                    ))}
                </div>
            </div>
        </div>
    );
}

export default LatestTxCard;

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import {
    getSingleTransactionKind,
    getTransactionKind,
    getTransferTransaction,
} from 'sui.js';

import Longtext from '../../components/longtext/Longtext';
import Search from '../../components/search/Search';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import ErrorResult from '../error-result/ErrorResult';

import type { CertifiedTransaction, GetTxnDigestsResponse } from 'sui.js';

import styles from './RecentTxCard.module.css';

const initState = {
    loadState: 'pending',
    latestTx: [],
};

const getRecentTransactions = async (txNum: number) => {
    try {
        // Get the latest transactions
        // TODO add batch transaction kind
        // TODO sui.js to get the latest transactions meta data
        const transactions = await rpc
            .getRecentTransactions(txNum)
            .then((res: GetTxnDigestsResponse) => res);
        const txLatest = await Promise.all(
            transactions.map(async (tx) => {
                return await rpc
                    .getTransaction(tx[1])
                    .then((res: CertifiedTransaction) => {
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
                            block: tx[0],
                            txId: tx[1],
                            // success: txData ? true : false,
                            kind: txKind,
                            From: res.data.sender,
                            ...(recipient
                                ? {
                                      To: recipient,
                                  }
                                : {}),
                        };
                    })
                    .catch((err) => {
                        console.error(
                            'Failed to get transaction details for txn digest',
                            tx[1],
                            err
                        );
                        return false;
                    });
            })
        );
        // Remove failed transactions and sort by block number
        return txLatest
            .filter((itm) => itm)
            .sort((a: any, b: any) => b.block - a.block);
    } catch (error) {
        throw error;
    }
};

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
                });
            });

        return () => {
            isMounted = false;
        };
    }, []);
    if (!isLoaded) {
        return (
            <div className={theme.textresults}>
                <div className={styles.content}>Loading...</div>
            </div>
        );
    }

    if (!isLoaded && results.loadState === 'fail') {
        return (
            <ErrorResult
                id="latestTx"
                errorMsg="There was an issue getting the latest transaction"
            />
        );
    }

    return (
        <div className={styles.txlatestesults}>
            <div className={styles.txcardgrid}>
                <h3>Latest Transaction</h3>
            </div>
            <div className={styles.txsearch}>{isLoaded && <Search />}</div>
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
                        <div className={styles.txtype}>Tx Type</div>
                        <div className={styles.txgas}>Gas</div>
                        <div className={styles.txadd}>Sender & Receiver</div>
                    </div>
                    {results.latestTx.map((tx: any, index: number) => (
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
                            <div className={styles.txgas}> 10</div>
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

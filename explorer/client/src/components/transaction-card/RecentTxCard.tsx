// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect, useState, useContext } from 'react';
import { Link, useSearchParams } from 'react-router-dom';

import Longtext from '../../components/longtext/Longtext';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import {
    DefaultRpcClient as rpc,
    type Network,
    getDataOnTxDigests,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { getAllMockTransaction } from '../../utils/static/searchUtil';
import { truncate } from '../../utils/stringUtils';
import ErrorResult from '../error-result/ErrorResult';
import Pagination from '../pagination/Pagination';

import type {
    GetTxnDigestsResponse,
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

function generateStartEndRange(
    txCount: number,
    txNum: number,
    pageNum?: number
): { startGatewayTxSeqNumber: number; endGatewayTxSeqNumber: number } {
    // Pagination pageNum from query params - default to 0; No negative values
    const txPaged = pageNum && pageNum > 0 ? pageNum - 1 : 0;
    const endGatewayTxSeqNumber: number = txCount - txNum * txPaged;
    const tempStartGatewayTxSeqNumber: number = endGatewayTxSeqNumber - txNum;
    // If startGatewayTxSeqNumber is less than 0, then set it 1 the first transaction sequence number
    const startGatewayTxSeqNumber: number =
        tempStartGatewayTxSeqNumber > 0 ? tempStartGatewayTxSeqNumber : 1;
    return {
        startGatewayTxSeqNumber,
        endGatewayTxSeqNumber,
    };
}

async function getRecentTransactions(
    network: Network | string,
    totalTx: number,
    txNum: number,
    pageNum?: number
): Promise<TxnData[]> {
    try {
        // Get the latest transactions

        // Instead of getRecentTransactions, use getTransactionCount
        // then use getTransactionDigestsInRange using the totalTx as the start totalTx sequence number - txNum as the end sequence number
        // Get the total number of transactions, then use as the start and end values for the getTransactionDigestsInRange
        const { endGatewayTxSeqNumber, startGatewayTxSeqNumber } =
            generateStartEndRange(totalTx, txNum, pageNum);

        // TODO: Add error page
        // If paged tx value is less than 0, out of range
        if (endGatewayTxSeqNumber < 0) {
            throw new Error('Invalid transaction number');
        }
        return (await rpc(network)
            .getTransactionDigestsInRange(
                startGatewayTxSeqNumber,
                endGatewayTxSeqNumber
            )
            .then((res: GetTxnDigestsResponse) =>
                getDataOnTxDigests(network, res)
            )) as TxnData[];
    } catch (error) {
        throw error;
    }
}

function LatestTxView({
    results,
}: {
    results: { loadState: string; latestTx: TxnData[] };
}) {
    const [network] = useContext(NetworkContext);
    return (
        <div className={styles.txlatestesults}>
            <div className={styles.txcardgrid}>
                <h3>Latest Transactions on {network}</h3>
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
                                {tx.status === 'success' ? '\u2714' : '\u2716'}
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

function LatestTxCardStatic({ count }: { count: number }) {
    const latestTx = getAllMockTransaction().map((tx) => ({
        ...tx,
        status: tx.status as ExecutionStatusType,
        kind: tx.kind as TransactionKindName,
    }));
    const [searchParams] = useSearchParams();
    const pagedNum: number = parseInt(searchParams.get('p') || '1', 10);

    const results = {
        loadState: 'loaded',
        latestTx: latestTx,
    };
    return (
        <>
            <LatestTxView results={results} />
            <Pagination totalTxCount={count} txNum={pagedNum} />
        </>
    );
}

function LatestTxCardAPI({ count }: { count: number }) {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    const [searchParams] = useSearchParams();
    const [txNumPerPage] = useState(15);
    useEffect(() => {
        let isMounted = true;
        const pagedNum: number = parseInt(searchParams.get('p') || '1', 10);
        getRecentTransactions(network, count, txNumPerPage, pagedNum)
            .then(async (resp: any) => {
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
    }, [count, network, searchParams, txNumPerPage]);

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
        <>
            <LatestTxView results={results} />
            <Pagination totalTxCount={count} txNum={txNumPerPage} />
        </>
    );
}

const LatestTxCard = ({ count }: { count: number }) =>
    IS_STATIC_ENV ? (
        <LatestTxCardStatic count={count} />
    ) : (
        <LatestTxCardAPI count={count} />
    );

export default LatestTxCard;

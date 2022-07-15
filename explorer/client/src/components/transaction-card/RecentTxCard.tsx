// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import cl from 'classnames';
import { useEffect, useState, useContext, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import { ReactComponent as ContentForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import TableCard from '../../components/table/TableCard';
import TabFooter from '../../components/tabs/TabFooter';
import Tabs from '../../components/tabs/Tabs';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import {
    DefaultRpcClient as rpc,
    type Network,
    getDataOnTxDigests,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { numberSuffix } from '../../utils/numberUtil';
import { getAllMockTransaction } from '../../utils/static/searchUtil';
import { truncate } from '../../utils/stringUtils';
import { timeAgo } from '../../utils/timeUtils';
import ErrorResult from '../error-result/ErrorResult';

import type {
    GetTxnDigestsResponse,
    ExecutionStatusType,
    TransactionKindName,
} from '@mysten/sui.js';

import styles from './RecentTxCard.module.css';

const TRUNCATE_LENGTH = 10;
const NUMBER_OF_TX_PER_PAGE = 15;

const initState: {
    loadState: string;
    latestTx: TxnData[];
    totalTxcount?: number;
} = {
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
    timestamp_ms?: number;
};

function generateStartEndRange(
    txCount: number,
    txNum: number,
    pageNum?: number
): { startGatewayTxSeqNumber: number; endGatewayTxSeqNumber: number } {
    // Pagination pageNum from query params - default to 0; No negative values
    const txPaged = pageNum && pageNum > 0 ? pageNum - 1 : 0;
    const endGatewayTxSeqNumber = txCount - txNum * txPaged;
    const startGatewayTxSeqNumber = Math.max(endGatewayTxSeqNumber - txNum, 0);
    return {
        startGatewayTxSeqNumber,
        endGatewayTxSeqNumber,
    };
}

async function getRecentTransactionsInRange(
    network: Network | string,
    totalTx: number,
    txNum: number,
    pageNum?: number
): Promise<TxnData[]> {
    try {
        // Get the transactions within a Range

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

// This converts the results into a format suitable for the table:
const convertResults = (inputResults: {
    loadState: string;
    latestTx: TxnData[];
    totalTxcount?: number;
}) => ({
    data: inputResults.latestTx.map((txn) => ({
        date: `${timeAgo(txn.timestamp_ms, undefined, true)} `,
        transactionId: [
            {
                url: txn.txId,
                name: truncate(txn.txId, TRUNCATE_LENGTH),
                category: 'transactions',
                isLink: true,
                copy: false,
            },
        ],
        addresses: [
            {
                url: txn.From,
                name: truncate(txn.From, TRUNCATE_LENGTH),
                category: 'addresses',
                isLink: true,
                copy: false,
            },
            ...(txn.To
                ? [
                      {
                          url: txn.To,
                          name: truncate(txn.To, TRUNCATE_LENGTH),
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

        gas: numberSuffix(txn.txGas),
    })),
    columns: [
        {
            headerLabel: 'Date',
            accessorKey: 'date',
        },
        {
            headerLabel: 'Type',
            accessorKey: 'txTypes',
        },
        {
            headerLabel: 'Transactions ID',
            accessorKey: 'transactionId',
        },
        {
            headerLabel: 'Addresses',
            accessorKey: 'addresses',
        },
        {
            headerLabel: 'Gas',
            accessorKey: 'gas',
        },
    ],
});

// 1) Sui Explorer decides between Static (-> 2A) or Live API (-> 2B) mode:
const LatestTxCard = () =>
    IS_STATIC_ENV ? <LatestTxCardStatic /> : <LatestTxCardAPI />;

// 2A) Static mode has no footer and pulls from the static dataset:
function LatestTxCardStatic() {
    const latestTx = getAllMockTransaction().map((tx) => ({
        ...tx,
        status: tx.status as ExecutionStatusType,
        kind: tx.kind as TransactionKindName,
    }));

    const results = {
        loadState: 'loaded',
        latestTx: latestTx,
    };

    return (
        <div className={styles.txlatestresults}>
            <Tabs selected={0}>
                <div title="Transactions">
                    <TableCard tabledata={convertResults(results)} />
                </div>
            </Tabs>
        </div>
    );
}

// 2B) In Live API mode under the inital load,
// either a page is not specified in the URL (-> 2BA) or the page is specified (-> 2BB).

function LatestTxCardAPI() {
    const [searchParams] = useSearchParams();
    const pagedNum: number = parseInt(searchParams.get('p') || '1', 10);

    return pagedNum > 1 ? (
        // A page is specified in the URL that is greater than 1:
        <LatestTxCardAPIWP pagedNum={pagedNum} />
    ) : (
        // A page is not specified:
        <LatestTxCardAPIInitialLoad />
    );
}

// 2BA) When no page is specified, the Explorer first displays the most recent transactions
// and then displays the footer when the count is found (-> 3):
function LatestTxCardAPIInitialLoad() {
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    const [count, setCount] = useState<'TBC' | number>('TBC');

    //Whenever network changes, we get a new list of recent tx:
    useEffect(() => {
        setResults(initState);
        rpc(network)
            .getRecentTransactions(NUMBER_OF_TX_PER_PAGE)
            .then((res: GetTxnDigestsResponse) =>
                getDataOnTxDigests(network, res)
            )
            .then((resp) =>
                setResults({
                    loadState: 'loaded',
                    latestTx: resp as TxnData[],
                })
            )
            .catch((err) => {
                setResults({
                    ...initState,
                    loadState: 'fail',
                });
                console.error(
                    'Encountered error when fetching recent transactions',
                    err
                );
            });
    }, [network]);

    //Whenever the network changes we initially close the footer, find the count and then re-open with new count:
    useEffect(() => {
        setCount('TBC');
        rpc(network)
            .getTotalTransactionNumber()
            .then((resp: number) => setCount(resp));
    }, [network]);

    if (results.loadState === 'pending') {
        return (
            <div className={theme.textresults}>
                <div className={styles.content}>Loading...</div>
            </div>
        );
    }

    if (results.loadState === 'fail') {
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

    return <LatestTxAPIView results={results} txCount={count} />;
}

// 2BB) When a page is specified the website waits for both the count and transaction data before displaying:
function LatestTxCardAPIWP({ pagedNum }: { pagedNum: number }) {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    const [count, setCount] = useState<'TBC' | number>('TBC');

    useEffect(() => {
        let isMounted = true;

        rpc(network)
            .getTotalTransactionNumber()
            .then((resp: number) => {
                setCount(resp);
                return getRecentTransactionsInRange(
                    network,
                    resp,
                    NUMBER_OF_TX_PER_PAGE,
                    pagedNum
                );
            })
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
                console.error(
                    'Encountered error when fetching recent transactions',
                    err
                );
            });

        return () => {
            isMounted = false;
        };
    }, [network, pagedNum]);

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
            <LatestTxAPIView results={results} txCount={count} />
        </>
    );
}

// 3) On the initial load, the most recent transactions are displayed.
// When the count has been found, this is displayed also.
// When the user clicks 'More Transactions', getTransactionsInRange is used
// to find the next list of recent transactions

function LatestTxAPIView({
    results,
    txCount,
}: {
    results: { loadState: string; latestTx: TxnData[] };
    txCount: 'TBC' | number;
}) {
    const [searchParams, setSearchParams] = useSearchParams();
    const pageParam = parseInt(searchParams.get('p') || '1', 10);
    const [showNextPage, setShowNextPage] = useState(false);

    const [network] = useContext(NetworkContext);

    //Initialize results to be the most recent transactions:
    const [newResults, setResults] = useState(results);

    useEffect(() => {
        if (txCount === 'TBC') return;
        setShowNextPage(NUMBER_OF_TX_PER_PAGE < txCount);
    }, [txCount]);

    const changePage = useCallback(() => {
        const nextpage = pageParam + (showNextPage ? 1 : 0);
        setSearchParams({ p: nextpage.toString() });
        setShowNextPage(Math.ceil(NUMBER_OF_TX_PER_PAGE * nextpage) < txCount);

        getRecentTransactionsInRange(
            network,
            txCount as number,
            NUMBER_OF_TX_PER_PAGE,
            nextpage
        )
            .then(async (resp: any) => {
                setResults({
                    loadState: 'loaded',
                    latestTx: resp,
                });
            })
            .catch((err) => {
                console.error(
                    'Encountered error when fetching next transactions page',
                    err
                );
            });
    }, [network, pageParam, txCount, showNextPage, setSearchParams]);

    //TODO update initial state and match the latestTx table data
    const defaultActiveTab = 0;
    const tabsFooter = {
        stats: {
            count: txCount || 0,
            stats_text: 'total transactions',
        },
    };
    return (
        <div className={styles.txlatestresults}>
            <Tabs selected={defaultActiveTab}>
                <div title="Transactions">
                    <TableCard tabledata={convertResults(newResults)} />
                    {txCount !== 'TBC' && (
                        <TabFooter stats={tabsFooter.stats}>
                            {showNextPage ? (
                                <button
                                    type="button"
                                    className={styles.moretxbtn}
                                    onClick={changePage}
                                >
                                    More Transactions{' '}
                                    <ContentForwardArrowDark />
                                </button>
                            ) : (
                                <></>
                            )}
                        </TabFooter>
                    )}
                </div>
            </Tabs>
        </div>
    );
}

export default LatestTxCard;

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import cl from 'classnames';
import { useEffect, useState, useContext } from 'react';
import { useSearchParams, Link } from 'react-router-dom';

import { ReactComponent as ContentForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import TableCard from '../../components/table/TableCard';
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
import Pagination from '../pagination/Pagination';

import type {
    GetTxnDigestsResponse,
    ExecutionStatusType,
    TransactionKindName,
} from '@mysten/sui.js';

import styles from './RecentTxCard.module.css';

const TRUNCATE_LENGTH = 10;
const NUMBER_OF_TX_PER_PAGE = 20;
const DEFAULT_PAGI_TYPE = 'more button';

type PaginationType = 'more button' | 'pagination' | 'none';

const initState: {
    loadState: string;
    latestTx: TxnData[];
    totalTxcount?: number;
    txPerPage?: number;
    truncateLength?: number;
    paginationtype?: PaginationType;
} = {
    loadState: 'pending',
    latestTx: [],
    totalTxcount: 0,
    txPerPage: NUMBER_OF_TX_PER_PAGE,
    truncateLength: TRUNCATE_LENGTH,
    paginationtype: 'pagination',
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

// Static data for development and testing
const getRecentTransactionsStatic = (): Promise<TxnData[]> => {
    return new Promise((resolve) => {
        setTimeout(() => {
            const latestTx = getAllMockTransaction().map((tx) => ({
                ...tx,
                status: tx.status as ExecutionStatusType,
                kind: tx.kind as TransactionKindName,
            }));
            resolve(latestTx as TxnData[]);
        }, 500);
    });
};

// TOD0: Optimize this method to use fewer API calls. Move the total tx count to this component.
async function getRecentTransactions(
    network: Network | string,
    totalTx: number,
    txNum: number,
    pageNum?: number
): Promise<TxnData[]> {
    try {
        // If static env, use static data
        if (IS_STATIC_ENV) {
            return getRecentTransactionsStatic();
        }
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

type RecentTx = {
    count?: number;
    paginationtype?: PaginationType;
    txPerPage?: number;
    truncateLength?: number;
};

function LatestTxCard({ ...data }: RecentTx) {
    const {
        count = 0,
        truncateLength = TRUNCATE_LENGTH,
        paginationtype = DEFAULT_PAGI_TYPE,
    } = data;

    const [txPerPage, setTxPerPage] = useState(
        data.txPerPage || NUMBER_OF_TX_PER_PAGE
    );

    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    const [searchParams, setSearchParams] = useSearchParams();

    const [pageIndex, setpageIndex] = useState(
        parseInt(searchParams.get('p') || '1', 10) || 1
    );

    useEffect(() => {
        let isMounted = true;

        // If pageIndex is greater than maxTxPage, set to maxTxPage
        const maxTxPage = Math.ceil(count / txPerPage);
        const pg = pageIndex > maxTxPage ? maxTxPage : pageIndex;
        setpageIndex(pg);

        pageIndex > 1
            ? setSearchParams({ p: pg.toString() })
            : setSearchParams({});

        getRecentTransactions(network, count, txPerPage, pg)
            .then(async (resp: any) => {
                if (isMounted) {
                    setIsLoaded(true);
                }
                setResults({
                    loadState: 'loaded',
                    latestTx: resp,
                    totalTxcount: count,
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
                Sentry.captureException(err);
            });
        return () => {
            isMounted = false;
        };
    }, [count, network, pageIndex, paginationtype, setSearchParams, txPerPage]);

    // update the page index when the user clicks on the pagination buttons

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

    //TODO update initial state and match the latestTx table data
    const defaultActiveTab = 0;
    const recentTx = {
        data: results.latestTx.map((txn) => ({
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

            gas: numberSuffix(txn.txGas),
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
                headerLabel: 'Gas',
                accessorKey: 'gas',
            },
        ],
    };

    const tabsFooter = {
        stats: {
            count,
            stats_text: 'Total transactions',
        },
    };

    return (
        <div className={cl(styles.txlatestresults, styles[paginationtype])}>
            <Tabs selected={defaultActiveTab}>
                <div title="Transactions">
                    <TableCard tabledata={recentTx} />
                    {paginationtype !== 'none' ? (
                        paginationtype === 'pagination' ? (
                            <Pagination
                                totalItems={count}
                                itemsPerPage={txPerPage}
                                updateItemsPerPage={setTxPerPage}
                                onPagiChangeFn={setpageIndex}
                                currentPage={pageIndex}
                                stats={tabsFooter.stats}
                            />
                        ) : (
                            <Link className={styles.moretxbtn} to={`/`}>
                                More Transactions <ContentForwardArrowDark />
                            </Link>
                        )
                    ) : (
                        <></>
                    )}
                </div>
            </Tabs>
        </div>
    );
}

export default LatestTxCard;

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect, useState, useContext } from 'react';
import { useSearchParams, Link } from 'react-router-dom';

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
import Pagination from '../pagination/Pagination';

import type {
    GetTxnDigestsResponse,
    ExecutionStatusType,
    TransactionKindName,
} from '@mysten/sui.js';

import styles from './RecentTxCard.module.css';

const TRUNCATE_LENGTH = 10;
const NUMBER_OF_TX_PER_PAGE = 15;
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

// Pass Props txPerPage, truncateLength, paginationtype to the component so that this component can be used for both the Home Page and trnasaction page
// TODO - rework this - gets confusing
function LatestTxView({
    results,
}: {
    results: {
        loadState: string;
        latestTx: TxnData[];
        totalTxcount?: number;
        txPerPage?: number;
        truncateLength?: number;
        paginationtype?: PaginationType;
    };
}) {
    const totalCount = results.totalTxcount || 1;
    const txPerPage = results.txPerPage || NUMBER_OF_TX_PER_PAGE;
    const truncateLength = results.truncateLength || TRUNCATE_LENGTH;
    const paginationtype = results.paginationtype || DEFAULT_PAGI_TYPE;

    //TODO update initial state and match the latestTx table data
    const defaultActiveTab = 0;
    const recentTx = {
        data: results.latestTx.map((txn) => ({
            date: `${timeAgo(txn.timestamp_ms, undefined, true)} `,
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
            count: totalCount || 0,
            stats_text: 'Total transactions',
        },
    };

    return (
        <div className={cl(styles.txlatestresults, styles[paginationtype])}>
            <Tabs selected={defaultActiveTab}>
                <div title="Transactions">
                    <TableCard tabledata={recentTx} />
                    <TabFooter stats={tabsFooter.stats}>
                        {paginationtype !== 'none' ? (
                            paginationtype === 'pagination' ? (
                                <Pagination
                                    totalTxCount={totalCount}
                                    txNum={txPerPage}
                                />
                            ) : (
                                <Link className={styles.moretxbtn} to={`/`}>
                                    More Transactions{' '}
                                    <ContentForwardArrowDark />
                                </Link>
                            )
                        ) : (
                            <></>
                        )}
                    </TabFooter>
                </div>
            </Tabs>
        </div>
    );
}

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
        <>
            <LatestTxView results={results} />
        </>
    );
}

type RecentTx = {
    count?: number;
    paginationtype?: PaginationType;
    txPerPage?: number;
    truncateLength?: number;
};

function LatestTxCardAPI({ ...data }: RecentTx) {
    const {
        count = 0,
        txPerPage = NUMBER_OF_TX_PER_PAGE,
        truncateLength = TRUNCATE_LENGTH,
        paginationtype = DEFAULT_PAGI_TYPE,
    } = data;
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    const [searchParams] = useSearchParams();

    useEffect(() => {
        let isMounted = true;
        const pagedNum: number = parseInt(searchParams.get('p') || '1', 10);
        getRecentTransactions(network, count, txPerPage, pagedNum)
            .then(async (resp: any) => {
                if (isMounted) {
                    setIsLoaded(true);
                }
                setResults({
                    loadState: 'loaded',
                    latestTx: resp,
                    totalTxcount: count,
                    txPerPage: txPerPage,
                    truncateLength: truncateLength,
                    paginationtype,
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
    }, [
        count,
        network,
        paginationtype,
        searchParams,
        truncateLength,
        txPerPage,
    ]);

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
        </>
    );
}

// Provide option to show pagination or not, so we can reuse this component for both homepage and the transactions page
const LatestTxCard = ({ ...data }: RecentTx) =>
    IS_STATIC_ENV ? <LatestTxCardStatic /> : <LatestTxCardAPI {...data} />;

export default LatestTxCard;

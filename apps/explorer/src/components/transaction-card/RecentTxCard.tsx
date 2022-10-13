// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type GetTxnDigestsResponse,
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import cl from 'clsx';
import { useEffect, useState, useContext, useCallback } from 'react';
import { useSearchParams, Link } from 'react-router-dom';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import TabFooter from '../../components/tabs/TabFooter';
import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { getAllMockTransaction } from '../../utils/static/searchUtil';
import ErrorResult from '../error-result/ErrorResult';
import Pagination from '../pagination/Pagination';
import {
    type TxnData,
    genTableDataFromTxData,
    getDataOnTxDigests,
} from './TxCardUtils';

import styles from './RecentTxCard.module.css';

import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard, type TableType } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

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

async function getTransactionCount(network: Network | string): Promise<number> {
    return rpc(network).getTotalTransactionNumber();
}

type RecentTx = {
    count?: number;
    paginationtype?: PaginationType;
    txPerPage?: number;
    truncateLength?: number;
};

function LatestTxCard({ ...data }: RecentTx) {
    const {
        truncateLength = TRUNCATE_LENGTH,
        paginationtype = DEFAULT_PAGI_TYPE,
    } = data;

    const [txPerPage, setTxPerPage] = useState(
        data.txPerPage || NUMBER_OF_TX_PER_PAGE
    );

    const [results, setResults] = useState(initState);
    const [recentTx, setRecentTx] = useState<null | TableType>(null);
    const [txCount, setTxCount] = useState({ loadState: 'pending', data: 0 });

    const [network] = useContext(NetworkContext);
    const [searchParams, setSearchParams] = useSearchParams();

    const [pageIndex, setPageIndex] = useState(
        parseInt(searchParams.get('p') || '1', 10) || 1
    );

    const handlePageChange = useCallback(
        (newPage: number) => {
            setPageIndex(newPage);
            setSearchParams({ p: newPage.toString() });
        },
        [setSearchParams]
    );

    const stats = {
        count: txCount.data,
        stats_text: 'Total transactions',
        loadState: txCount.loadState,
    };

    const PaginationWithStatsOrStatsWithLink =
        paginationtype === 'pagination' ? (
            <Pagination
                totalItems={txCount.data}
                itemsPerPage={txPerPage}
                updateItemsPerPage={setTxPerPage}
                onPagiChangeFn={handlePageChange}
                currentPage={pageIndex}
                stats={stats}
            />
        ) : (
            <TabFooter stats={stats}>
                <Link className={styles.moretxbtn} to="/transactions">
                    <div>More Transactions</div> <ArrowRight />
                </Link>
            </TabFooter>
        );
    // update the page index when the user clicks on the pagination buttons
    useEffect(() => {
        getTransactionCount(network)
            .then((resp: number) => {
                setTxCount({
                    loadState: 'loaded',
                    data: resp,
                });

                return resp;
            })
            .catch((err) => {
                setTxCount({
                    loadState: 'fail',
                    data: 0,
                });
                setResults({
                    ...initState,
                    loadState: 'fail',
                });

                console.error(
                    'Encountered error when fetching transaction count',
                    err
                );
                return null;
            })
            .then((count: number | null) => {
                if (count) {
                    // If pageIndex is greater than maxTxPage, set to maxTxPage
                    const maxTxPage = Math.ceil(count / txPerPage);
                    const pg = pageIndex > maxTxPage ? maxTxPage : pageIndex;

                    getRecentTransactions(network, count, txPerPage, pg)
                        .then(async (resp: any) => {
                            setResults({
                                loadState: 'loaded',
                                latestTx: resp,
                                totalTxcount: count,
                            });

                            if (resp.length > 0) {
                                setRecentTx(
                                    genTableDataFromTxData(resp, truncateLength)
                                );
                            }
                        })
                        .catch((err) => {
                            setResults({
                                ...initState,
                                loadState: 'fail',
                            });
                            console.error(
                                'Encountered error when fetching recent transactions',
                                err
                            );
                            Sentry.captureException(err);
                        });
                }
            });
    }, [network, pageIndex, setSearchParams, txPerPage, truncateLength]);

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

    return (
        <div className={cl(styles.txlatestresults, styles[paginationtype])}>
            <TabGroup size="lg">
                <TabList>
                    <Tab>Transactions</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        {recentTx ? (
                            <TableCard tabledata={recentTx} />
                        ) : (
                            <PlaceholderTable
                                rowCount={15}
                                rowHeight="16px"
                                colHeadings={[
                                    'Time',
                                    'Type',
                                    'Transaction ID',
                                    'Addresses',
                                    'Amount',
                                    'Gas',
                                ]}
                                colWidths={[
                                    '85px',
                                    '95px',
                                    '90px',
                                    '204px',
                                    '90px',
                                    '38px',
                                ]}
                            />
                        )}
                        {paginationtype !== 'none' &&
                            PaginationWithStatsOrStatsWithLink}
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}

export default LatestTxCard;

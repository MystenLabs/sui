// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useContext } from 'react';

import ErrorResult from '../../components/error-result/ErrorResult';
import LatestTxCard from '../../components/transaction-card/RecentTxCard';
import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './Transactions.module.css';

const initState = { count: 0, loadState: 'pending' };
const TXN_PER_PAGE = 20;
const TRUNCATE_LENGTH = 45;
// Moved this method to the Home.tsx file so getTotalTransactionNumber can be called once across the entire component.
async function getTransactionCount(network: Network | string): Promise<number> {
    return rpc(network).getTotalTransactionNumber();
}

function TransactionsStatic() {
    const [count] = useState(500);
    return (
        <div data-testid="home-page" id="home" className={styles.home}>
            <LatestTxCard count={count} />
        </div>
    );
}

function TransactionsAPI() {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        let isMounted = true;
        getTransactionCount(network)
            .then((resp: number) => {
                if (isMounted) {
                    setIsLoaded(true);
                }
                setResults({
                    loadState: 'loaded',
                    count: resp,
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
    }, [network]);

    if (results.loadState === 'pending') {
        return <div className={styles.gray}>loading...</div>;
    }

    if (!isLoaded && results.loadState === 'fail') {
        return (
            <ErrorResult
                id=""
                errorMsg="Error getting total transaction count"
            />
        );
    }
    return (
        <div
            data-testid="transaction-page"
            id="transaction"
            className={styles.container}
        >
            <LatestTxCard
                count={results.count}
                txPerPage={TXN_PER_PAGE}
                paginationtype="pagination"
                truncateLength={TRUNCATE_LENGTH}
            />
        </div>
    );
}

const Transactions = () =>
    IS_STATIC_ENV ? <TransactionsStatic /> : <TransactionsAPI />;

export default Transactions;

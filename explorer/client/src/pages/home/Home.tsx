// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useContext } from 'react';

import ErrorResult from '../../components/error-result/ErrorResult';
import LastestTxCard from '../../components/transaction-card/RecentTxCard';
import TxCountCard from '../../components/transaction-count/TxCountCard';
import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './Home.module.css';

const initState = { count: 0, loadState: 'pending' };

// Moved this method to the Home.tsx file so getTotalTransactionNumber can be called once across the entire component.
async function getTransactionCount(network: Network | string): Promise<number> {
    return rpc(network).getTotalTransactionNumber();
}

function HomeStatic() {
    const [count] = useState(500);
    return (
        <div data-testid="home-page" id="home" className={styles.home}>
            <LastestTxCard count={count} />
            <TxCountCard count={count} />
        </div>
    );
}

function HomeAPI() {
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
        <div data-testid="home-page" id="home" className={styles.home}>
            <LastestTxCard count={results.count} />
            <TxCountCard count={results.count} />
        </div>
    );
}

const Home = () => (IS_STATIC_ENV ? <HomeStatic /> : <HomeAPI />);

export default Home;

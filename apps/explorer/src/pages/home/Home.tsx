// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';
import { useEffect, useState, useContext, lazy, Suspense } from 'react';

import ErrorResult from '../../components/error-result/ErrorResult';
import {
    TopValidatorsCardAPI,
    TopValidatorsCardStatic,
} from '../../components/top-validators-card/TopValidatorsCard';
import LastestTxCard from '../../components/transaction-card/RecentTxCard';
import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './Home.module.css';

const ValidatorMap = lazy(
    () => import('../../components/validator-map/ValidatorMap')
);

const initState = { count: 0, loadState: 'pending' };
const TXN_PER_PAGE = 15;
// Moved this method to the Home.tsx file so getTotalTransactionNumber can be called once across the entire component.
async function getTransactionCount(network: Network | string): Promise<number> {
    return rpc(network).getTotalTransactionNumber();
}

function HomeStatic() {
    const [count] = useState(500);
    return (
        <div
            data-testid="home-page"
            id="home"
            className={cl([styles.home, styles.container])}
        >
            <section className="left-item">
                <LastestTxCard count={count} />
            </section>
            <section className="right-item">
                <TopValidatorsCardStatic />
            </section>
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
        return <div />;
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
            data-testid="home-page"
            id="home"
            className={cl([styles.home, styles.container])}
        >
            <section className="left-item">
                <LastestTxCard
                    txPerPage={TXN_PER_PAGE}
                    count={results.count}
                    paginationtype="more button"
                />
            </section>
            <section className="right-item">
                <TopValidatorsCardAPI />
                <Suspense fallback={null}>
                    <ValidatorMap />
                </Suspense>
            </section>
        </div>
    );
}

const Home = () => (IS_STATIC_ENV ? <HomeStatic /> : <HomeAPI />);

export default Home;

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';
import { lazy, Suspense } from 'react';

import {
    TopValidatorsCardAPI,
    TopValidatorsCardStatic,
} from '../../components/top-validators-card/TopValidatorsCard';
import LastestTxCard from '../../components/transaction-card/RecentTxCard';
import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './Home.module.css';

const ValidatorMap = lazy(
    () => import('../../components/validator-map/ValidatorMap')
);

const TXN_PER_PAGE = 15;

function HomeStatic() {
    return (
        <div
            data-testid="home-page"
            id="home"
            className={cl([styles.home, styles.container])}
        >
            <section className="left-item">
                <LastestTxCard />
            </section>
            <section className="right-item">
                <TopValidatorsCardStatic />
            </section>
        </div>
    );
}

function HomeAPI() {
    return (
        <div
            data-testid="home-page"
            id="home"
            className={cl([styles.home, styles.container])}
        >
            <section className="left-item">
                <LastestTxCard
                    txPerPage={TXN_PER_PAGE}
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

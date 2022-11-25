// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';
import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { RecentModulesCard } from '../../components/recent-packages-card/RecentPackagesCard';
import {
    TopValidatorsCardStatic,
    TopValidatorsCardAPI,
} from '../../components/top-validators-card/TopValidatorsCard';
import { LatestTxCard } from '../../components/transaction-card/RecentTxCard';
import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './Home.module.css';

import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const ValidatorMap = lazy(
    () => import('../../components/validator-map/ValidatorMap')
);

const TXN_PER_PAGE = 25;

function HomeStatic() {
    return (
        <div
            data-testid="home-page"
            id="home"
            className={cl([styles.home, styles.container])}
        >
            <section className="left-item">
                <LatestTxCard />
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
            <section className="left-item mb-4 md:mb-0">
                <ErrorBoundary>
                    <LatestTxCard
                        txPerPage={TXN_PER_PAGE}
                        paginationtype="more button"
                    />
                </ErrorBoundary>
            </section>
            <section className="right-item flex flex-col gap-10 md:gap-12">
                <ErrorBoundary>
                    <TopValidatorsCardAPI />
                </ErrorBoundary>
                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <ValidatorMap />
                    </Suspense>
                </ErrorBoundary>
                <div>
                    <TabGroup>
                        <TabList>
                            <Tab>Recent Packages</Tab>
                        </TabList>
                        <TabPanels>
                            <TabPanel>
                                <ErrorBoundary>
                                    <RecentModulesCard />
                                </ErrorBoundary>
                            </TabPanel>
                        </TabPanels>
                    </TabGroup>
                </div>
            </section>
        </div>
    );
}

function Home() {
    return IS_STATIC_ENV ? <HomeStatic /> : <HomeAPI />;
}

export default Home;

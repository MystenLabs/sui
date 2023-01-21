// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { RecentModulesCard } from '../../components/recent-packages-card/RecentPackagesCard';
import { TopValidatorsCard } from '../../components/top-validators-card/TopValidatorsCard';
import { LatestTxCard } from '../../components/transaction-card/RecentTxCard';

import { HomeMetrics } from '~/components/HomeMetrics';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const NodeMap = lazy(() => import('../../components/node-map'));

const TXN_PER_PAGE = 25;

function Home() {
    return (
        <div
            data-testid="home-page"
            id="home"
            className="mx-auto grid grid-cols-1 gap-2 bg-white md:grid-cols-2"
        >
            <section className="mb-4 md:mb-0">
                <ErrorBoundary>
                    <LatestTxCard
                        txPerPage={TXN_PER_PAGE}
                        paginationtype="more button"
                    />
                    <HomeMetrics />
                </ErrorBoundary>
            </section>
            <section className="flex flex-col gap-10 md:gap-12">
                <div data-testid="validators-table">
                    <TabGroup>
                        <TabList>
                            <Tab>Validators</Tab>
                        </TabList>
                        <TabPanels>
                            <TabPanel>
                                <ErrorBoundary>
                                    <TopValidatorsCard limit={10} showIcon />
                                </ErrorBoundary>
                            </TabPanel>
                        </TabPanels>
                    </TabGroup>
                </div>
                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <NodeMap />
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

export default Home;

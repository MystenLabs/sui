// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
// import { RecentModulesCard } from '../../components/recent-packages-card/RecentPackagesCard';
import { TopValidatorsCard } from '../../components/top-validators-card/TopValidatorsCard';

import { Activity } from '~/components/Activity';
import { HomeMetrics } from '~/components/HomeMetrics';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const NodeMap = lazy(() => import('../../components/node-map'));

const TRANSACTIONS_LIMIT = 25;

function Home() {
    return (
        <div
            data-testid="home-page"
            // NOTE: The gap-y isn't used currently, but added for consistency when we eventually use grid layouts more naturally.
            className="grid grid-cols-1 gap-y-10 gap-x-12 md:grid-cols-2"
        >
            <div className="flex flex-col gap-10">
                <ErrorBoundary>
                    <HomeMetrics />
                </ErrorBoundary>

                <ErrorBoundary>
                    <Activity
                        initialLimit={TRANSACTIONS_LIMIT}
                        disablePagination
                    />
                </ErrorBoundary>
            </div>

            <div className="flex flex-col gap-10">
                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <NodeMap minHeight={280} />
                    </Suspense>
                </ErrorBoundary>
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
                {/* <div>
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
                </div> */}
            </div>
        </div>
    );
}

export default Home;

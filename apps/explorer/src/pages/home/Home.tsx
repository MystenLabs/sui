// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
// import { RecentModulesCard } from '../../components/recent-packages-card/RecentPackagesCard';
import { TopValidatorsCard } from '../../components/top-validators-card/TopValidatorsCard';

import { Activity } from '~/components/Activity';
import { GasPriceCard } from '~/components/GasPriceCard';
import { HomeMetrics } from '~/components/HomeMetrics';
import { Checkpoint } from '~/components/HomeMetrics/Checkpoint';
import { CurrentEpoch } from '~/components/HomeMetrics/CurrentEpoch';
import { NetworkTPS } from '~/components/HomeMetrics/NetworkTPS';
import { OnTheNetwork } from '~/components/HomeMetrics/OnTheNetwork';
import { SuiTokenCard } from '~/components/SuiTokenCard';
import { Card } from '~/ui/Card';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const NodeMap = lazy(() => import('../../components/node-map'));

const TRANSACTIONS_LIMIT = 25;

function Home() {
    // const isHomePageRedesignEnabled = useFeatureIsOn(
    //     'explorer-home-page-redesign'
    // );
    const isHomePageRedesignEnabled = true;

    return isHomePageRedesignEnabled ? (
        <div
            data-testid="home-page"
            className="grid grid-cols-1 gap-x-4 gap-y-4 grid-areas-condensedHomePage md:grid-cols-[200px,1fr] md:grid-areas-homePage lg:grid-cols-[200px,454px,1fr] lg:grid-areas-fullHomePage"
        >
            <div className="grid-in-tps">
                <NetworkTPS />
            </div>
            <div className="grid-in-sui-token">
                <SuiTokenCard />
            </div>
            <div className="overflow-hidden grid-in-network">
                <OnTheNetwork />
            </div>
            <div className="grid-in-epoch">
                <CurrentEpoch />
            </div>
            <div className="grid-in-checkpoint">
                <Checkpoint />
            </div>
            <div className="grid-in-gas-price">
                <GasPriceCard />
            </div>
            <div className="grid-in-node-map">
                <ErrorBoundary>
                    <Suspense fallback={<Card height="full" />}>
                        <NodeMap minHeight="100%" />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div className="mt-5 grid-in-activity">
                <ErrorBoundary>
                    <Activity
                        initialLimit={TRANSACTIONS_LIMIT}
                        disablePagination
                    />
                </ErrorBoundary>
            </div>
            <div
                data-testid="validators-table"
                className="mt-5 grid-in-validator"
            >
                <TabGroup size="lg">
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
            <div className="mt-5 bg-gray-60 grid-in-packages">
                Popular packages
            </div>
        </div>
    ) : (
        <div
            data-testid="home-page"
            className="grid grid-cols-1 gap-x-12 gap-y-10 md:grid-cols-2"
        >
            <ErrorBoundary>
                <HomeMetrics />
            </ErrorBoundary>

            <ErrorBoundary>
                <Suspense fallback={<Card />}>
                    <NodeMap minHeight={280} />
                </Suspense>
            </ErrorBoundary>

            <ErrorBoundary>
                <Activity initialLimit={TRANSACTIONS_LIMIT} disablePagination />
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
    );
}

export default Home;

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useFeatureIsOn } from '@growthbook/growthbook-react';
import clsx from 'clsx';
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
    const isHomePageRedesignEnabled = useFeatureIsOn(
        'explorer-home-page-redesign'
    );
    const isSuiTokenCardEnabled = useFeatureIsOn('explorer-sui-token-card');

    return isHomePageRedesignEnabled ? (
        <div
            data-testid="home-page"
            className="grid grid-cols-1 gap-x-4 gap-y-4 md:grid-cols-[200px,1fr] lg:grid-cols-[200px,454px,1fr]"
        >
            <NetworkTPS />
            {isSuiTokenCardEnabled && <SuiTokenCard />}
            <div
                className={clsx('overflow-hidden md:col-span-full', {
                    'lg:col-span-2': !isSuiTokenCardEnabled,
                    'lg:col-auto': isSuiTokenCardEnabled,
                })}
            >
                <OnTheNetwork />
            </div>
            <CurrentEpoch />
            <div className="md:row-start-4 lg:row-start-3">
                <Checkpoint />
            </div>
            <div className="md:row-start-3 md:row-end-5 lg:row-start-2 lg:row-end-4">
                <GasPriceCard />
            </div>
            <div className="md:col-span-full lg:col-auto lg:row-start-2 lg:row-end-4">
                <ErrorBoundary>
                    <Suspense fallback={<Card height="full" />}>
                        <NodeMap minHeight="100%" />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div className="mt-5 md:col-span-full lg:col-span-2 lg:row-span-2">
                <ErrorBoundary>
                    <Activity
                        initialLimit={TRANSACTIONS_LIMIT}
                        disablePagination
                    />
                </ErrorBoundary>
            </div>
            <div
                data-testid="validators-table"
                className="mt-5 md:col-span-full lg:col-auto"
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
            {/* TODO: Add the popular packages component here :) */}
            {/* <div className="mt-5 bg-gray-60 md:col-span-full lg:col-auto">
                Popular packages
            </div> */}
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

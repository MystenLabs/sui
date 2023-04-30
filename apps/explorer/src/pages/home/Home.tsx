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
            className={clsx('home-page-grid-container', {
                'home-page-grid-container-with-sui-token':
                    isSuiTokenCardEnabled,
            })}
        >
            <div className="grid-area-tps">
                <NetworkTPS />
            </div>
            {isSuiTokenCardEnabled && (
                <div className="grid-area-sui-token">
                    <SuiTokenCard />
                </div>
            )}
            <div className="grid-area-network overflow-hidden">
                <OnTheNetwork />
            </div>
            <div className="grid-area-epoch">
                <CurrentEpoch />
            </div>
            <div className="grid-area-checkpoint">
                <Checkpoint />
            </div>
            <div className="grid-area-gas-price">
                <GasPriceCard />
            </div>
            <div className="grid-area-node-map h-[360px] xl:h-auto">
                <ErrorBoundary>
                    <Suspense fallback={<Card height="full" />}>
                        <NodeMap minHeight="100%" />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div className="grid-area-activity mt-5">
                <ErrorBoundary>
                    <Activity
                        initialLimit={TRANSACTIONS_LIMIT}
                        disablePagination
                    />
                </ErrorBoundary>
            </div>
            <div
                data-testid="validators-table"
                className="grid-area-validators mt-5"
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
            {/* <div className="grid-area-packages mt-5 bg-gray-60">
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

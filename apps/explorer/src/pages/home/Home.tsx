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
            <div style={{ gridArea: 'tps' }}>
                <NetworkTPS />
            </div>
            {isSuiTokenCardEnabled && (
                <div style={{ gridArea: 'sui-token' }}>
                    <SuiTokenCard />
                </div>
            )}
            <div style={{ gridArea: 'network' }} className="overflow-hidden">
                <OnTheNetwork />
            </div>
            <div style={{ gridArea: 'epoch' }}>
                <CurrentEpoch />
            </div>
            <div style={{ gridArea: 'checkpoint' }}>
                <Checkpoint />
            </div>
            <div style={{ gridArea: 'gas-price' }}>
                <GasPriceCard />
            </div>
            <div
                style={{ gridArea: 'node-map' }}
                className="h-[360px] xl:h-auto"
            >
                <ErrorBoundary>
                    <Suspense fallback={<Card height="full" />}>
                        <NodeMap minHeight="100%" />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div style={{ gridArea: 'activity' }} className="mt-5">
                <ErrorBoundary>
                    <Activity
                        initialLimit={TRANSACTIONS_LIMIT}
                        disablePagination
                    />
                </ErrorBoundary>
            </div>
            <div
                data-testid="validators-table"
                style={{ gridArea: 'validators' }}
                className="mt-5"
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
            {/* <div style={{ gridArea: 'packages' }} className="mt-5 bg-gray-60">
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { TopValidatorsCard } from '../../components/top-validators-card/TopValidatorsCard';
import { AccountsCardGraph } from '~/components/AccountCardGraph';
import { Activity } from '~/components/Activity';
import { CurrentEpoch, OnTheNetwork } from '~/components/HomeMetrics';
import { GradientContainer } from '~/components/Layout/GradientContainer';
import { TransactionsCardGraph } from '~/components/TransactionsCardGraph';
import { TopPackagesCard } from '~/components/top-packages/TopPackagesCard';
import { Card } from '~/ui/Card';
import { TabHeader } from '~/ui/Tabs';

const ValidatorMap = lazy(() => import('../../components/validator-map'));

const TRANSACTIONS_LIMIT = 25;

function Home() {
	return (
		<>
			<GradientContainer>
				<div className="home-page-grid-container-top">
					<div style={{ gridArea: 'network' }} className="overflow-hidden">
						<OnTheNetwork />
					</div>
					<div style={{ gridArea: 'epoch' }}>
						<CurrentEpoch />
					</div>
					<div style={{ gridArea: 'transactions' }}>
						<TransactionsCardGraph />
					</div>
					<div style={{ gridArea: 'accounts' }}>
						<AccountsCardGraph />
					</div>
				</div>
			</GradientContainer>
			<div className="home-page-grid-container-bottom">
				<div style={{ gridArea: 'activity' }}>
					<ErrorBoundary>
						<Activity initialLimit={TRANSACTIONS_LIMIT} disablePagination />
					</ErrorBoundary>
				</div>
				<div style={{ gridArea: 'packages' }}>
					<TopPackagesCard />
				</div>
				<div data-testid="validators-table" style={{ gridArea: 'validators' }}>
					<TabHeader title="Validators">
						<ErrorBoundary>
							<TopValidatorsCard limit={10} showIcon />
						</ErrorBoundary>
					</TabHeader>
				</div>
				<div style={{ gridArea: 'node-map' }} className="min-h-[360px]">
					<ErrorBoundary>
						<Suspense fallback={<Card height="full" />}>
							<ValidatorMap minHeight="100%" />
						</Suspense>
					</ErrorBoundary>
				</div>
			</div>
		</>
	);
}

export default Home;

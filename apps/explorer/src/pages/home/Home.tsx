// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { lazy, Suspense } from 'react';

import { AccountsCardGraph } from '~/components/AccountCardGraph';
import { Activity } from '~/components/Activity';
import { CurrentEpoch, OnTheNetwork } from '~/components/HomeMetrics';
import { PageLayout } from '~/components/Layout/PageLayout';
import { SuiTokenCard } from '~/components/SuiTokenCard';
import { TransactionsCardGraph } from '~/components/TransactionsCardGraph';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TopPackagesCard } from '~/components/top-packages/TopPackagesCard';
import { TopValidatorsCard } from '~/components/top-validators-card/TopValidatorsCard';
import { useNetwork } from '~/context';
import { Card } from '~/ui/Card';
import { TabHeader } from '~/ui/Tabs';
import { Network } from '~/utils/api/DefaultRpcClient';
import { Image } from '~/ui/image/Image';
import { Heading, Text } from '@mysten/ui';
import { ArrowRight12, Sui, SuiLogoTxt } from '@mysten/icons';
import { ButtonOrLink } from '~/ui/utils/ButtonOrLink';
import suiscanImg from '../../assets/explorer-suiscan.jpg';
import suiscanImg2x from '../../assets/explorer-suiscan@2x.jpg';
import suivisionImg from '../../assets/explorer-suivision.jpg';
import suivisionImg2x from '../../assets/explorer-suivision@2x.jpg';

const ValidatorMap = lazy(() => import('../../components/validator-map'));

const TRANSACTIONS_LIMIT = 25;

function ExternalExplorerLink({ type }: { type: 'suiscan' | 'suivision' }) {
	const href = type === 'suiscan' ? 'https://suiscan.xyz' : 'https://suivision.xyz';
	const src = type === 'suiscan' ? suiscanImg : suivisionImg;
	const srcSet = type === 'suiscan' ? suiscanImg2x : suivisionImg2x;

	return (
		<div className="relative overflow-hidden rounded-3xl border border-gray-45 transition duration-300 ease-in-out hover:shadow-lg">
			<ButtonOrLink href={href} target="_blank" rel="noopener noreferrer">
				<Image src={src} srcSet={srcSet} />
			</ButtonOrLink>
			<div className="absolute bottom-10 left-1/2 right-0 flex -translate-x-1/2 sm:w-96">
				<ButtonOrLink
					className="flex w-full items-center justify-center gap-2 rounded-3xl bg-sui-dark px-3 py-2"
					href={href}
					target="_blank"
					rel="noopener noreferrer"
				>
					<Text variant="body/semibold" color="white">
						{type === 'suiscan' ? 'Visit Suiscan.xyz' : 'Visit Suivision.xyz'}
					</Text>
					<ArrowRight12 className="h-3 w-3 -rotate-45 text-white" />
				</ButtonOrLink>
			</div>
		</div>
	);
}

function Home() {
	const [network] = useNetwork();
	const isSuiTokenCardEnabled = network === Network.MAINNET;
	// const enableExplorerRedirect = useFeatureIsOn('explorer-redirect');
	// TODO: Change back to use feature flag before merging
	const enableExplorerRedirect = true;

	if (enableExplorerRedirect) {
		return (
			<PageLayout
				header={
					<section
						className="mb-20 flex flex-col items-center justify-center gap-5 px-5 py-20 text-center"
						style={{
							background: 'linear-gradient(159deg, #FAF8D2 50.65%, #F7DFD5 86.82%)',
						}}
					>
						<div className="flex items-center gap-1">
							<Sui className="h-11 w-9" />
							<SuiLogoTxt className="h-7 w-11" />
						</div>

						<Heading variant="heading2/bold">
							Experience two amazing blockchain explorers on Sui!
						</Heading>
					</section>
				}
				content={
					<section className="flex flex-col justify-center gap-10 sm:flex-row">
						<ExternalExplorerLink type="suivision" />
						<ExternalExplorerLink type="suiscan" />
					</section>
				}
			/>
		);
	}

	return (
		<PageLayout
			gradient={{
				content: (
					<div
						data-testid="home-page"
						className={clsx('home-page-grid-container-top', isSuiTokenCardEnabled && 'with-token')}
					>
						<div style={{ gridArea: 'network' }} className="overflow-hidden">
							<OnTheNetwork />
						</div>
						<div style={{ gridArea: 'epoch' }}>
							<CurrentEpoch />
						</div>
						{isSuiTokenCardEnabled ? (
							<div style={{ gridArea: 'token' }}>
								<SuiTokenCard />
							</div>
						) : null}
						<div style={{ gridArea: 'transactions' }}>
							<TransactionsCardGraph />
						</div>
						<div style={{ gridArea: 'accounts' }}>
							<AccountsCardGraph />
						</div>
					</div>
				),
				size: 'lg',
			}}
			content={
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
					<div
						style={{ gridArea: 'node-map' }}
						className="min-h-[320px] sm:min-h-[380px] lg:min-h-[460px] xl:min-h-[520px]"
					>
						<ErrorBoundary>
							<Suspense fallback={<Card height="full" />}>
								<ValidatorMap minHeight="100%" />
							</Suspense>
						</ErrorBoundary>
					</div>
				</div>
			}
		/>
	);
}

export default Home;

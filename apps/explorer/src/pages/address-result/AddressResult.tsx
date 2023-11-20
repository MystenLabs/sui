// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiNSName, useResolveSuiNSAddress, useResolveSuiNSName } from '@mysten/core';
import { Domain32 } from '@mysten/icons';
import { LoadingIndicator } from '@mysten/ui';
import { useParams } from 'react-router-dom';

import { PageLayout } from '~/components/Layout/PageLayout';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TransactionsForAddress } from '~/components/transactions/TransactionsForAddress';
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { Divider } from '~/ui/Divider';
import { PageHeader } from '~/ui/PageHeader';
import { LOCAL_STORAGE_SPLIT_PANE_KEYS, SplitPanes } from '~/ui/SplitPanes';
import { TabHeader, TabsList, TabsTrigger } from '~/ui/Tabs';
import { TotalStaked } from './TotalStaked';

const LEFT_RIGHT_PANEL_MIN_SIZE = 30;
const TOP_PANEL_MIN_SIZE = 20;

function AddressResultPageHeader({ address, loading }: { address: string; loading?: boolean }) {
	const { data: domainName, isLoading } = useResolveSuiNSName(address);

	return (
		<PageHeader
			loading={loading || isLoading}
			type="Address"
			title={address}
			subtitle={domainName}
			before={<Domain32 className="h-6 w-6 text-steel-darker sm:h-10 sm:w-10" />}
			after={<TotalStaked address={address} />}
		/>
	);
}

function SuiNSAddressResultPageHeader({ name }: { name: string }) {
	const { data: address, isLoading } = useResolveSuiNSAddress(name);

	return <AddressResultPageHeader address={address ?? name} loading={isLoading} />;
}

function AddressResult({ address }: { address: string }) {
	const isMediumOrAbove = useBreakpoint('md');

	const leftPane = {
		panel: <OwnedCoins id={address} />,
		minSize: LEFT_RIGHT_PANEL_MIN_SIZE,
		defaultSize: LEFT_RIGHT_PANEL_MIN_SIZE,
	};

	const rightPane = {
		panel: <OwnedObjects id={address} fullHeight={isMediumOrAbove} />,
		minSize: LEFT_RIGHT_PANEL_MIN_SIZE,
	};

	const topPane = {
		panel: (
			<div className="flex h-full flex-col justify-between">
				<ErrorBoundary>
					{isMediumOrAbove ? (
						<SplitPanes
							autoSaveId={LOCAL_STORAGE_SPLIT_PANE_KEYS.ADDRESS_VIEW_HORIZONTAL}
							dividerSize="none"
							splitPanels={[leftPane, rightPane]}
							direction="horizontal"
						/>
					) : (
						<>
							{leftPane.panel}
							<div className="my-8">
								<Divider />
							</div>
							{rightPane.panel}
						</>
					)}
				</ErrorBoundary>
			</div>
		),
		minSize: TOP_PANEL_MIN_SIZE,
	};

	const bottomPane = {
		panel: (
			<div className="flex h-full flex-col pt-12">
				<TabsList>
					<TabsTrigger value="tab">Transaction Blocks</TabsTrigger>
				</TabsList>

				<ErrorBoundary>
					<div data-testid="tx" className="relative mt-4 h-full min-h-14 overflow-auto">
						<TransactionsForAddress address={address} type="address" />
					</div>
				</ErrorBoundary>

				<div className="mt-0.5">
					<Divider />
				</div>
			</div>
		),
	};

	return (
		<TabHeader title="Owned Objects" noGap>
			{isMediumOrAbove ? (
				<div className="h-300">
					<SplitPanes
						autoSaveId={LOCAL_STORAGE_SPLIT_PANE_KEYS.ADDRESS_VIEW_VERTICAL}
						dividerSize="none"
						splitPanels={[topPane, bottomPane]}
						direction="vertical"
					/>
				</div>
			) : (
				<>
					{topPane.panel}
					<div className="mt-5">
						<Divider />
					</div>
					{bottomPane.panel}
				</>
			)}
		</TabHeader>
	);
}

function SuiNSAddressResult({ name }: { name: string }) {
	const { isFetched, data } = useResolveSuiNSAddress(name);

	if (!isFetched) {
		return <LoadingIndicator />;
	}

	// Fall back into just trying to load the name as an address anyway:
	return <AddressResult address={data ?? name} />;
}

export default function AddressResultPage() {
	const { id } = useParams();
	const isSuiNSAddress = isSuiNSName(id!);

	return (
		<PageLayout
			gradient={{
				size: 'md',
				content: isSuiNSAddress ? (
					<SuiNSAddressResultPageHeader name={id!} />
				) : (
					<AddressResultPageHeader address={id!} />
				),
			}}
			content={isSuiNSAddress ? <SuiNSAddressResult name={id!} /> : <AddressResult address={id!} />}
		/>
	);
}

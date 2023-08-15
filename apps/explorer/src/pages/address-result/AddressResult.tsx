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
import { SplitPanes } from '~/ui/SplitPanes';
import { TabHeader } from '~/ui/Tabs';

function AddressResultPageHeader({ address, loading }: { address: string; loading?: boolean }) {
	const { data: domainName, isFetching } = useResolveSuiNSName(address);

	return (
		<PageHeader
			loading={loading || isFetching}
			type="Address"
			title={address}
			subtitle={domainName}
			before={<Domain32 className="h-6 w-6 text-steel-darker sm:h-10 sm:w-10" />}
		/>
	);
}

function SuiNSAddressResultPageHeader({ name }: { name: string }) {
	const { data: address, isFetching } = useResolveSuiNSAddress(name);

	return <AddressResultPageHeader address={address ?? name} loading={isFetching} />;
}

function AddressResult({ address }: { address: string }) {
	const isMediumOrAbove = useBreakpoint('md');

	const leftPane = {
		panel: (
			<div className="flex-1 overflow-hidden md:pr-7">
				<OwnedCoins id={address} />
			</div>
		),
		minSize: 30,
	};

	const rightPane = {
		panel: <OwnedObjects id={address} />,
		minSize: 30,
	};

	return (
		<div className="space-y-12">
			<div>
				<TabHeader title="Owned Objects" noGap>
					<ErrorBoundary>
						{isMediumOrAbove ? (
							<SplitPanes splitPanels={[leftPane, rightPane]} direction="horizontal" />
						) : (
							<>
								{leftPane.panel}
								<div className="my-8">
									<Divider />
								</div>
								{rightPane.panel}
							</>
						)}
						<Divider />
					</ErrorBoundary>
				</TabHeader>
			</div>

			<div>
				<ErrorBoundary>
					<div className="mt-2">
						<TransactionsForAddress address={address} type="address" />
					</div>
				</ErrorBoundary>
			</div>
		</div>
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

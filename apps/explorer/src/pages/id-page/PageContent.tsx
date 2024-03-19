// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import { Banner } from '~/ui/Banner';
import { Divider } from '~/ui/Divider';
import { FieldsContent } from '~/pages/object-result/views/TokenView';
import { TabHeader } from '~/ui/Tabs';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TransactionsForAddressTable } from '~/components/transactions/TransactionsForAddress';
import TransactionBlocksForAddress from '~/components/TransactionBlocksForAddress';
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { LOCAL_STORAGE_SPLIT_PANE_KEYS, SplitPanes } from '~/ui/SplitPanes';
import { useSuiClient } from '@mysten/dapp-kit';
import { useQuery } from '@tanstack/react-query';

const LEFT_RIGHT_PANEL_MIN_SIZE = 30;

function OwnedObjectsSection({ address }: { address: string }) {
	const isMediumOrAbove = useBreakpoint('md');

	const leftPane = {
		panel: (
			<div className="mb-5 h-full md:h-coinsAndAssetsContainer">
				<OwnedCoins id={address} />
			</div>
		),
		minSize: LEFT_RIGHT_PANEL_MIN_SIZE,
		defaultSize: LEFT_RIGHT_PANEL_MIN_SIZE,
	};

	const rightPane = {
		panel: (
			<div className="mb-5 h-full md:h-coinsAndAssetsContainer">
				<OwnedObjects id={address} />
			</div>
		),
		minSize: LEFT_RIGHT_PANEL_MIN_SIZE,
	};

	return (
		<TabHeader title="Owned Objects" noGap>
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
		</TabHeader>
	);
}

function TransactionsSection({ address, isObject }: { address: string; isObject: boolean }) {
	const client = useSuiClient();

	const {
		data: transactionsForAddressData,
		isPending,
		isError,
	} = useQuery({
		queryKey: ['transactions-for-address', address],
		queryFn: () =>
			client.queryTransactionBlocks({
				filter: {
					FromAndToAddress: {
						from: address,
						to: address,
					},
				},
				order: 'descending',
				limit: 100,
				options: {
					showEffects: true,
					showInput: true,
				},
			}),
		enabled: !isObject,
	});

	return (
		<ErrorBoundary>
			{isObject ? (
				<TransactionBlocksForAddress address={address} />
			) : (
				<TransactionsForAddressTable
					data={transactionsForAddressData?.data ?? []}
					isPending={isPending}
					isError={isError}
					address={address}
				/>
			)}
		</ErrorBoundary>
	);
}

export function PageContent({ address, error }: { address: string; error?: Error | null }) {
	const { data } = useGetObject(address);
	const isObject = !!data?.data;

	if (error) {
		return (
			<Banner variant="error" spacing="lg" fullWidth>
				Data could not be extracted on the following specified address ID: {address}
			</Banner>
		);
	}

	return (
		<div>
			<section>
				<OwnedObjectsSection address={address} />
			</section>

			<Divider />

			{isObject && (
				<section className="mt-14">
					<FieldsContent objectId={address} />
				</section>
			)}

			<section className="mt-14">
				<TabHeader title="Transaction Blocks">
					<TransactionsSection address={address} isObject={isObject} />
				</TabHeader>
			</section>
		</div>
	);
}

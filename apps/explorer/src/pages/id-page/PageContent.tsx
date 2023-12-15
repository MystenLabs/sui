// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Banner } from '~/ui/Banner';
import { Divider } from '~/ui/Divider';
import { FieldsContent } from '~/pages/object-result/views/TokenView';
import { TabHeader } from '~/ui/Tabs';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TransactionsForAddress } from '~/components/transactions/TransactionsForAddress';
import TransactionBlocksForAddress, {
	FILTER_VALUES,
	FiltersControl,
} from '~/components/TransactionBlocksForAddress';
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { LOCAL_STORAGE_SPLIT_PANE_KEYS, SplitPanes } from '~/ui/SplitPanes';
import { Modules } from '~/pages/id-page/Modules';
import { type DataType } from '~/pages/object-result/ObjectResultType';
import { useState } from 'react';

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

export function PageContent({
	address,
	error,
	pageType,
	data,
}: {
	address: string;
	pageType: 'Package' | 'Object' | 'Address';
	data?: DataType | null;
	error?: Error | null;
}) {
	const [filterValue, setFilterValue] = useState(FILTER_VALUES.CHANGED);

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

			{pageType === 'Object' && (
				<section className="mt-14">
					<FieldsContent objectId={address} />
				</section>
			)}

			{pageType === 'Package' && data && (
				<section className="mt-14">
					<Modules data={data} />
				</section>
			)}

			<section className="mt-14">
				<TabHeader
					title="Transaction Blocks"
					after={
						pageType !== 'Address' && (
							<div>
								<FiltersControl filterValue={filterValue} setFilterValue={setFilterValue} />
							</div>
						)
					}
				>
					<ErrorBoundary>
						{pageType === 'Address' ? (
							<div data-testid="address-txn-table">
								<TransactionsForAddress type="address" address={address} />
							</div>
						) : (
							<div data-testid="object-txn-table">
								<TransactionBlocksForAddress address={address} filter={filterValue} />
							</div>
						)}
					</ErrorBoundary>
				</TabHeader>
			</section>
		</div>
	);
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { TabHeader } from '~/ui/Tabs';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { LOCAL_STORAGE_SPLIT_PANE_KEYS, SplitPanes } from '~/ui/SplitPanes';
import { Divider } from '~/ui/Divider';

const LEFT_RIGHT_PANEL_MIN_SIZE = 30;

export function OwnedObjectsSection({ address }: { address: string }) {
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents } from '@mysten/core';
import { formatAddress } from '@mysten/sui.js/utils';
import { useSearchParams, Link } from 'react-router-dom';

import { useActiveAddress } from '_app/hooks/useActiveAddress';
import { LabelValueItem } from '_src/ui/app/components/LabelValueItem';
import { LabelValuesContainer } from '_src/ui/app/components/LabelValuesContainer';
import { ErrorBoundary } from '_src/ui/app/components/error-boundary';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import Loading from '_src/ui/app/components/loading';
import { NFTDisplayCard } from '_src/ui/app/components/nft-display';
import PageTitle from '_src/ui/app/shared/PageTitle';
import { Collapsible } from '_src/ui/app/shared/collapse';

function KioskDetailsPage() {
	const [searchParams] = useSearchParams();
	const kioskId = searchParams.get('kioskId');
	const accountAddress = useActiveAddress();
	const { data: kioskData, isLoading } = useGetKioskContents(accountAddress);
	const kiosk = kioskData?.kiosks.get(kioskId!);
	const items = kiosk?.items;

	return (
		<div className="flex flex-1 flex-col flex-nowrap gap-3.75 mb-10">
			<PageTitle title="Kiosk" back />
			<Loading loading={isLoading}>
				{!items?.length ? (
					<div className="flex flex-1 items-center self-center text-caption font-semibold text-steel-darker">
						Kiosk is empty
					</div>
				) : (
					<>
						<div className="grid grid-cols-3 gap-3 items-center justify-center mb-auto">
							{items.map((item) => (
								<Link
									to={`/nft-details?${new URLSearchParams({
										objectId: item.data?.objectId!,
									}).toString()}`}
									key={item.data?.objectId}
									className="no-underline"
								>
									<ErrorBoundary>
										<NFTDisplayCard
											objectId={item.data?.objectId!}
											size="md"
											showLabel
											animateHover
											borderRadius="xl"
											isLocked={item?.isLocked}
										/>
									</ErrorBoundary>
								</Link>
							))}
						</div>
					</>
				)}
				<Collapsible defaultOpen title="Details">
					<LabelValuesContainer>
						<LabelValueItem label="Number of Items" value={items?.length || '0'} />
						<LabelValueItem
							label="Kiosk ID"
							value={
								<ExplorerLink
									className="text-hero-dark no-underline font-mono"
									objectID={kioskId!}
									type={ExplorerLinkType.object}
								>
									{formatAddress(kioskId!)}
								</ExplorerLink>
							}
						/>
					</LabelValuesContainer>
				</Collapsible>
			</Loading>
		</div>
	);
}

export default KioskDetailsPage;

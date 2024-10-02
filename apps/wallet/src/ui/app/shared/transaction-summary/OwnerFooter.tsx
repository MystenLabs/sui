// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { useActiveAddress } from '_src/ui/app/hooks';
import { formatAddress, isValidSuiAddress } from '@mysten/sui/utils';

import { Text } from '../text';
import { SummaryCardFooter } from './Card';

export function OwnerFooter({ owner, ownerType }: { owner?: string; ownerType?: string }) {
	const address = useActiveAddress();
	const isOwner = address === owner;

	if (!owner) return null;
	const display =
		ownerType === 'Shared'
			? 'Shared'
			: isValidSuiAddress(owner)
				? isOwner
					? 'You'
					: formatAddress(owner)
				: owner;

	return (
		<SummaryCardFooter>
			<Text variant="pBody" weight="medium" color="steel-dark">
				Owner
			</Text>
			<div className="flex justify-end">
				{isOwner ? (
					<Text variant="body" weight="medium" color="hero-dark">
						{display}
					</Text>
				) : (
					<ExplorerLink
						type={ExplorerLinkType.address}
						title={owner}
						address={owner}
						className="text-hero-dark text-body font-medium no-underline font-mono"
					>
						{display}
					</ExplorerLink>
				)}
			</div>
		</SummaryCardFooter>
	);
}

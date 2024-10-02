// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { NftImage } from '_src/ui/app/components/nft-display/NftImage';
import { type SuiObjectChangeWithDisplay } from '@mysten/core';
import { formatAddress } from '@mysten/sui/utils';

import { Text } from '../../../text';

export function ObjectChangeDisplay({ change }: { change: SuiObjectChangeWithDisplay }) {
	const display = change?.display?.data;
	const objectId = 'objectId' in change && change?.objectId;

	if (!display) return null;
	return (
		<div className="relative group w-32 cursor-pointer whitespace-nowrap min-w-min">
			<NftImage
				size="md"
				name={display.name ?? ''}
				borderRadius="xl"
				src={display.image_url ?? ''}
			/>
			{objectId && (
				<div className="absolute bottom-2 full left-1/2 transition-opacity group-hover:opacity-100 opacity-0 -translate-x-1/2 justify-center rounded-lg bg-white/90 px-2 py-1">
					<ExplorerLink
						type={ExplorerLinkType.object}
						objectID={objectId}
						className="text-hero-dark no-underline"
					>
						<Text variant="pBodySmall" truncate mono>
							{formatAddress(objectId)}
						</Text>
					</ExplorerLink>
				</div>
			)}
		</div>
	);
}

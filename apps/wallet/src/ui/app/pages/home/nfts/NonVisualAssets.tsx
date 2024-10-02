// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { Text } from '_src/ui/app/shared/text';
import { type SuiObjectData } from '@mysten/sui/client';
import { formatAddress, parseStructTag } from '@mysten/sui/utils';

export default function NonVisualAssets({ items }: { items: SuiObjectData[] }) {
	return (
		<div className="flex flex-col items-center gap-4 w-full flex-1">
			{items?.length ? (
				<div className="flex flex-col flex-wrap w-full divide-y divide-solid divide-gray-40 divide-x-0 gap-3">
					{items.map((item) => {
						const { address, module, name } = parseStructTag(item.type!);
						return (
							<div className="grid grid-cols-3 pt-3" key={item.objectId}>
								<ExplorerLink
									className="text-hero-dark no-underline"
									objectID={item.objectId!}
									type={ExplorerLinkType.object}
								>
									<Text variant="pBody">{formatAddress(item.objectId!)}</Text>
								</ExplorerLink>

								<div className="break-all col-span-2">
									<Text
										variant="pBodySmall"
										weight="normal"
										mono
										color="steel"
										title={item.type ?? ''}
									>
										{`${formatAddress(address)}::${module}::${name}`}
									</Text>
								</div>
							</div>
						);
					})}
				</div>
			) : (
				<div className="flex flex-1 items-center self-center text-caption font-semibold text-steel-darker">
					No Assets found
				</div>
			)}
		</div>
	);
}

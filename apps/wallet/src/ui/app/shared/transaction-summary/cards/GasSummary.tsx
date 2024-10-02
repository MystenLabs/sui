// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { useActiveAddress } from '_src/ui/app/hooks';
import { GAS_TYPE_ARG } from '_src/ui/app/redux/slices/sui-objects/Coin';
import { useFormatCoin, type GasSummaryType } from '@mysten/core';
import { formatAddress } from '@mysten/sui/utils';

import { Text } from '../../text';

export function GasSummary({ gasSummary }: { gasSummary?: GasSummaryType }) {
	const [gas, symbol] = useFormatCoin(gasSummary?.totalGas, GAS_TYPE_ARG);
	const address = useActiveAddress();

	if (!gasSummary) return null;

	return (
		<div className="bg-white relative flex flex-col shadow-card-soft rounded-2xl">
			<div className="bg-gray-40 rounded-t-2xl py-2.5 px-4">
				<Text color="steel-darker" variant="captionSmall" weight="semibold">
					Gas Fees
				</Text>
			</div>
			<div className="flex flex-col items-center gap-2 w-full px-4 py-3">
				<div className="flex w-full items-center justify-start">
					{address === gasSummary?.owner && (
						<div className="mr-auto">
							<Text color="steel-dark" variant="pBody" weight="medium">
								You Paid
							</Text>
						</div>
					)}
					<Text color="steel-darker" variant="pBody" weight="medium">
						{gasSummary?.isSponsored ? '0' : gas} {symbol}
					</Text>
				</div>
				{gasSummary?.isSponsored && gasSummary.owner && (
					<>
						<div className="flex w-full justify-between">
							<Text color="steel-dark" variant="pBody" weight="medium">
								Paid by Sponsor
							</Text>
							<Text color="steel-darker" variant="pBody" weight="medium">
								{gas} {symbol}
							</Text>
						</div>
						<div className="flex w-full justify-between">
							<Text color="steel-dark" variant="pBody" weight="medium">
								Sponsor
							</Text>
							<ExplorerLink
								type={ExplorerLinkType.address}
								address={gasSummary.owner}
								className="text-hero-dark no-underline"
							>
								<Text variant="pBodySmall" truncate mono>
									{formatAddress(gasSummary.owner)}
								</Text>
							</ExplorerLink>
						</div>
					</>
				)}
			</div>
		</div>
	);
}

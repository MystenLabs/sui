// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type BalanceChange } from '@mysten/sui/client';
import { useQuery } from '@tanstack/react-query';

import { useDryRunContext } from '../DryRunContext';
import { PreviewCard } from '../PreviewCard';
import { onChainAmountToFloat } from '../utils';

export function BalanceChanges({ changes }: { changes: BalanceChange[] }) {
	return (
		<div className="grid grid-cols-2 gap-4 even:bg-gray-900">
			{changes.map((change, index) => (
				<ChangedBalance key={index} change={change} />
			))}
		</div>
	);
}

function ChangedBalance({ change }: { change: BalanceChange }) {
	const { network, client } = useDryRunContext();

	const { data: coinMetadata } = useQuery({
		queryKey: [network, 'getCoinMetadata', change.coinType],
		queryFn: async () => {
			return await client.getCoinMetadata({
				coinType: change.coinType,
			});
		},
		enabled: !!change.coinType,
	});

	const amount = () => {
		if (!coinMetadata) return '-';
		const amt = onChainAmountToFloat(change.amount, coinMetadata.decimals);

		return `${amt && amt > 0.0 ? '+' : ''}${amt}`;
	};

	if (!coinMetadata) return <div>Loading...</div>;

	return (
		<PreviewCard.Root>
			<PreviewCard.Body>
				<>
					{coinMetadata.iconUrl && (
						<img
							src={coinMetadata.iconUrl as string}
							alt={coinMetadata.name}
							className="w-12 h-auto"
						/>
					)}
					<p>
						<span className={`${Number(amount()) > 0.0 ? 'text-green-300' : 'text-red-700'}`}>
							{amount()}{' '}
						</span>{' '}
						{coinMetadata.symbol}
						<span className="block text-sm">{change.coinType}</span>
					</p>
				</>
			</PreviewCard.Body>
			<PreviewCard.Footer owner={change.owner} />
		</PreviewCard.Root>
	);
}

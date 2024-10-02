// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DryRunTransactionBlockResponse, GasCostSummary } from '@mysten/sui/client';
import { ReactNode } from 'react';

import { useDryRunContext } from '../DryRunContext';
import { ObjectLink } from '../ObjectLink';
import { onChainAmountToFloat } from '../utils';

const calculateGas = (gas: GasCostSummary): string => {
	return (
		onChainAmountToFloat(
			(
				BigInt(gas.computationCost) +
				BigInt(gas.storageCost) -
				BigInt(gas.storageRebate)
			).toString(),
			9,
		)?.toString() || '-'
	);
};

export function Overview({ output }: { output: DryRunTransactionBlockResponse }) {
	const { network } = useDryRunContext();

	const metadata: Record<string, ReactNode> = {
		network,
		status:
			output.effects.status?.status === 'success'
				? '✅ Transaction dry run executed succesfully!'
				: output.effects.status?.status === 'failure'
					? '❌ Transaction failed to execute!'
					: null,

		sender: (
			<span className="flex gap-2 items-center">
				<ObjectLink
					owner={{
						AddressOwner: output.input.sender,
					}}
				/>
			</span>
		),
		epoch: output.effects.executedEpoch,
		gas: calculateGas(output.effects.gasUsed) + ' SUI',
	};

	return (
		<div className="border p-3 w-full rounded">
			{Object.entries(metadata).map(([key, value]) => (
				<div key={key} className="flex items-center gap-3 ">
					<span className="capitalize">{key}: </span>
					{value}
				</div>
			))}
		</div>
	);
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PreviewCard } from '../preview-effects/PreviewCard';
import { onChainAmountToFloat } from '../preview-effects/utils';
import { type Effects, type ReplayGasStatus, type ReplayInput } from './replay-types';
import { ReplayInputArgument } from './ReplayInputArgument';

export function ReplayOverview({
	effects,
	gasStatus,
	inputs,
}: {
	effects: Effects;
	gasStatus: ReplayGasStatus;
	inputs: ReplayInput[];
}) {
	const totalGasCost = () => {
		return (
			onChainAmountToFloat(
				(
					BigInt(effects.gasUsed.computationCost) +
					BigInt(effects.gasUsed.storageCost) -
					BigInt(effects.gasUsed.storageRebate)
				).toString(),
				9,
			)?.toString() || '-'
		);
	};

	return (
		<div>
			<div>
				<div className="px-2 py-2 m-1">
					<p>
						Execution Status:{' '}
						{effects.status.status === 'success' ? '.𖥔 ݁ ˖Success ݁ ˖ 𖥔' : 'Failure ❗'}
					</p>
					<p> Executed Epoch: {effects.executedEpoch}</p>
				</div>

				<PreviewCard.Root className="m-2">
					<PreviewCard.Header> Gas Cost </PreviewCard.Header>
					<PreviewCard.Body>
						<p>Total Gas Cost: {totalGasCost()} SUI</p>
						<p>Computation Cost: {onChainAmountToFloat(effects.gasUsed.computationCost, 9)} SUI</p>
						<p>Storage Cost: {onChainAmountToFloat(effects.gasUsed.storageCost, 9)} SUI</p>
						<p>Storage Rebate: {onChainAmountToFloat(effects.gasUsed.storageRebate, 9)} SUI</p>
						<p>
							Non-refundable Storage Fee:{' '}
							{onChainAmountToFloat(effects.gasUsed.nonRefundableStorageFee, 9)} SUI
						</p>
					</PreviewCard.Body>
				</PreviewCard.Root>
				<PreviewCard.Root className="m-2">
					<PreviewCard.Header> Gas Info </PreviewCard.Header>
					<PreviewCard.Body>
						<p>Gas Price: {gasStatus.V2.gasPrice} MIST </p>
						<p>Reference Gas Price: {gasStatus.V2.referenceGasPrice} MIST </p>
						<p>Max Gas Stack Height: {gasStatus.V2.gasStatus.stackHeightHighWaterMark} </p>
						<p>Max Gas Stack Size: {gasStatus.V2.gasStatus.stackSizeHighWaterMark} </p>
						<p>
							Number of Bytecode Instructions Executed:{' '}
							{gasStatus.V2.gasStatus.instructionsExecuted}{' '}
						</p>
					</PreviewCard.Body>
				</PreviewCard.Root>

				<PreviewCard.Root className="m-2">
					<PreviewCard.Header>Input Arguments</PreviewCard.Header>
					<PreviewCard.Body>
						<div className="text-sm max-h-[450px] overflow-y-auto grid grid-cols-1 gap-3">
							{inputs.map((input, index) => (
								<ReplayInputArgument input={input} key={index} />
							))}
						</div>
					</PreviewCard.Body>
				</PreviewCard.Root>
			</div>
			<p className="py-3 px-6 text-xs font-bold">1 MIST = 10^-9 SUI</p>
		</div>
	);
}

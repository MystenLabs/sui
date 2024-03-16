// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReplayLink } from '@/components/replay/ReplayLink';

import { PreviewCard } from '../preview-effects/PreviewCard';
import { Effects, EffectsObject } from './replay-types';
import { DeletedItem, EffectsItem } from './ReplayInputArgument';

export function ReplayEffects({ effects }: { effects: Effects }) {
	const output = [];
	if ('created' in effects) {
		output.push(effectsSection('Created', effects.created));
	}
	if ('mutated' in effects) {
		output.push(effectsSection('Mutated', effects.mutated));
	}
	if ('wrapped' in effects) {
		output.push(effectsSection('Wrapped', effects.wrapped));
	}
	if ('unwrapped' in effects) {
		output.push(effectsSection('Unwrapped', effects.unwrapped));
	}
	if ('deleted' in effects) {
		output.push(
			<div>
				<PreviewCard.Root className="m-2">
					<PreviewCard.Header> Deleted </PreviewCard.Header>
					<PreviewCard.Body>
						<div className="text-sm max-h-[450px] overflow-y-auto grid grid-cols-1 gap-3">
							{effects.deleted.map((ref, index) => (
								<DeletedItem input={ref} key={index} />
							))}
						</div>
					</PreviewCard.Body>
				</PreviewCard.Root>
			</div>,
		);
	}
	output.push(
		<div>
			<PreviewCard.Root className="m-2">
				<PreviewCard.Header> Dependencies </PreviewCard.Header>
				<PreviewCard.Body>
					<div className="text-sm max-h-[450px] overflow-y-auto grid grid-cols-1 gap-3">
						{effects.dependencies.map((dep) => (
							<ReplayLink id={dep} text={dep} />
						))}
					</div>
				</PreviewCard.Body>
			</PreviewCard.Root>
		</div>,
	);
	return output;
}

const effectsSection = (name: string, input: EffectsObject[]) => {
	return (
		<div>
			<PreviewCard.Root className="m-2">
				<PreviewCard.Header> {name} </PreviewCard.Header>
				<PreviewCard.Body>
					<div className="text-sm max-h-[450px] overflow-y-auto grid grid-cols-1 gap-3">
						{input.map((item, index) => (
							<EffectsItem input={item} key={index} />
						))}
					</div>
				</PreviewCard.Body>
			</PreviewCard.Root>
		</div>
	);
};

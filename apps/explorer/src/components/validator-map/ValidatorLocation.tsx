// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

import { type ValidatorMapValidator } from './types';

interface Props {
	validator: ValidatorMapValidator;
	projection: (loc: [number, number]) => [number, number] | null;
	onMouseOver(event: React.MouseEvent, countryCode?: string): void;
	onMouseOut(): void;
}

// NOTE: This should be tweaked based on the average number of nodes in a location:
const VALIDATOR_MULTIPLIER = 20;
const MIN_VALIDATOR_SIZE = 2.5;
const MAX_VALIDATOR_SIZE = 10;
const MAX_VALIDATOR_OPACITY = 0.5;

export function ValidatorLocation({ validator, projection, onMouseOut, onMouseOver }: Props) {
	const handleMouseOver = useCallback(
		(e: React.MouseEvent) => {
			validator && onMouseOver(e, validator.suiAddress);
		},
		[validator, onMouseOver],
	);

	if (!validator.ipInfo) return null;

	const position = projection(
		validator.ipInfo.loc
			.split(',')
			.reverse()
			.map((geo) => parseFloat(geo)) as [number, number],
	);

	const r = Math.max(
		Math.min(
			Math.floor(parseInt(validator.votingPower) / VALIDATOR_MULTIPLIER),
			MAX_VALIDATOR_SIZE,
		),
		MIN_VALIDATOR_SIZE,
	);

	if (!position) return null;

	const opacity = MAX_VALIDATOR_OPACITY - parseInt(validator.votingPower) / 1000;
	return (
		<g>
			<circle
				onMouseOver={handleMouseOver}
				onMouseMove={handleMouseOver}
				onMouseOut={onMouseOut}
				cx={position[0]}
				cy={position[1]}
				r={r}
				fill="#6FBCF0"
				opacity={opacity}
			/>
		</g>
	);
}

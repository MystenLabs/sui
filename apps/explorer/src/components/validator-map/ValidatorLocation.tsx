// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

import { type ValidatorWithLocation } from './types';

interface Props {
    validator: ValidatorWithLocation | null;
    projection: (loc: [number, number]) => [number, number] | null;
    onMouseOver(event: React.MouseEvent, countryCode?: string): void;
    onMouseOut(): void;
}

// NOTE: This should be tweaked based on the average number of nodes in a location:
const VALIDATOR_MULTIPLIER = 20;
const MIN_VALIDATOR_SIZE = 0.5;
const MAX_VALIDATOR_SIZE = 7;

export function ValidatorLocation({
    validator,
    projection,
    onMouseOut,
    onMouseOver,
}: Props) {
    const handleMouseOver = useCallback(
        (e: React.MouseEvent) => {
            validator && onMouseOver(e, validator.suiAddress);
        },
        [validator, onMouseOver]
    );

    if (!validator) {
        return null;
    }

    const position = projection(
        validator.loc
            .split(',')
            .reverse()
            .map((geo) => parseFloat(geo)) as [number, number]
    );

    const r = Math.max(
        Math.min(
            Math.floor(parseInt(validator.votingPower) / VALIDATOR_MULTIPLIER),
            MAX_VALIDATOR_SIZE
        ),
        MIN_VALIDATOR_SIZE
    );

    if (!position) return null;

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
                opacity={0.4}
            />
        </g>
    );
}

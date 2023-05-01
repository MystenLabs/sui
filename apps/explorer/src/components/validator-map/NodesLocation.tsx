// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';
import { ValidatorWithLocation, type NodeLocation } from './types';

interface Props {
    node: ValidatorWithLocation | null;
    projection: (loc: [number, number]) => [number, number] | null;
    onMouseOver(event: React.MouseEvent, countryCode?: string): void;
    onMouseOut(): void;
}

// NOTE: This should be tweaked based on the average number of nodes in a location:
const NODE_MULTIPLIER = 20;
const MIN_NODE_SIZE = 0.5;
const MAX_NODE_SIZE = 7;

export function NodesLocation({ node, projection, onMouseOut, onMouseOver }: Props) {
    if(!node){ 
        return null
    }
    const position = projection(
        node.loc
            .split(',')
            .reverse()
            .map((geo) => parseFloat(geo)) as [number, number]
    );

    const r = Math.max(
        Math.min(Math.floor(parseInt(node.votingPower) / NODE_MULTIPLIER), MAX_NODE_SIZE),
        MIN_NODE_SIZE
    );

    if (!position) return null;

    const handleMouseOver = useCallback(
        (e: React.MouseEvent) => {
            onMouseOver(e, node.suiAddress);
        },
        [node.votingPower, onMouseOver]
    );

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

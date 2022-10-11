// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type NodeLocation } from './types';

interface Props {
    node: NodeLocation;
    projection: (loc: [number, number]) => [number, number] | null;
}

// NOTE: This should be tweaked based on the average number of nodes in a location:
const NODE_MULTIPLIER = 3;
const MIN_NODE_SIZE = 2;
const MAX_NODE_SIZE = 7;

export function NodesLocation({ node, projection }: Props) {
    const position = projection(
        node.location
            .split(',')
            .reverse()
            .map((geo) => parseFloat(geo)) as [number, number]
    );

    const r = Math.max(
        Math.min(Math.floor(node.count / NODE_MULTIPLIER), MAX_NODE_SIZE),
        MIN_NODE_SIZE
    );

    if (!position) return null;

    return (
        <g style={{ pointerEvents: 'none' }}>
            <circle
                cx={position[0]}
                cy={position[1]}
                r={r}
                fill="#6FBCF0"
                opacity={0.4}
            />
        </g>
    );
}

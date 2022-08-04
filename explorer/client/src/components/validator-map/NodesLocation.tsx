// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";

import { type NodeLocation } from "./types";

interface Props {
	node: NodeLocation;
	projection: (loc: [number, number]) => [number, number] | null;
}

export function NodesLocation({ node, projection }: Props) {
	const position = projection(node.location);

	// TODO: Distribute based on node count.
	const [r] = useState(() => 5 + Math.floor(Math.random() * 7));

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

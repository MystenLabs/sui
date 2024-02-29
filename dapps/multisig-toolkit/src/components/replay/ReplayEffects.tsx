// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Mutated } from './replay-types';

export function ReplayEffects({ mutated }: { mutated: Mutated[] }) {
	return (
		<div>
			{mutated.map((item) => (
				<div>{JSON.stringify(item)}</div>
			))}
		</div>
	);
}

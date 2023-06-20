// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import type { SuiMoveAbilitySet } from '@mysten/sui.js';

export function useFunctionTypeArguments(typeArguments: SuiMoveAbilitySet[]) {
	return useMemo(
		() =>
			typeArguments.map(
				(aTypeArgument, index) =>
					`T${index}${
						aTypeArgument.abilities.length ? `: ${aTypeArgument.abilities.join(' + ')}` : ''
					}`,
			),
		[typeArguments],
	);
}

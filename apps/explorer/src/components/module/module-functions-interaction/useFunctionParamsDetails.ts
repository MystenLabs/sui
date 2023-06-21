// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { getNormalizedFunctionParameterTypeDetails } from '../utils';

import type { SuiMoveNormalizedType } from '@mysten/sui.js';

export function useFunctionParamsDetails(
	params: SuiMoveNormalizedType[],
	functionTypeArgNames?: string[],
) {
	return useMemo(
		() =>
			params
				.map((aParam) => getNormalizedFunctionParameterTypeDetails(aParam, functionTypeArgNames))
				.filter(({ isTxContext }) => !isTxContext),
		[params, functionTypeArgNames],
	);
}

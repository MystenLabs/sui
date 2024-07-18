// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	formatAmount as formatAmountNew,
	formatAmountParts as formatAmountPartsNew,
} from '@mysten/sui/utils';

/**
 * @deprecated Use '@mysten/sui/utils/formatAmountParts' instead.
 */
export function formatAmountParts(...args: Parameters<typeof formatAmountPartsNew>) {
	return formatAmountPartsNew(...args);
}

/**
 * @deprecated Use '@mysten/sui/utils/formatAmount' instead.
 */
export function formatAmount(...args: Parameters<typeof formatAmountNew>) {
	return formatAmountNew(...args);
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiMoveNormalizedType } from '../client/index.js';
import { extractStructTag } from '../types/index.js';

export function isTxContext(param: SuiMoveNormalizedType): boolean {
	const struct = extractStructTag(param)?.Struct;
	return (
		struct?.address === '0x2' && struct?.module === 'tx_context' && struct?.name === 'TxContext'
	);
}

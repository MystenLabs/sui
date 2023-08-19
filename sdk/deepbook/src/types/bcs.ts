// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui.js/bcs';

bcs.registerStructType('Order', {
	orderId: 'u64',
	clientOrderId: 'u64',
	price: 'u64',
	originalQuantity: 'u64',
	quantity: 'u64',
	isBid: 'bool',
	owner: 'address',
	expireTimestamp: 'u64',
	selfMatchingPrevention: 'u8',
});

export { bcs };

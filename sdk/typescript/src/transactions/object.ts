// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionObjectInput } from './Transaction.js';

export function createObjectMethods<T>(makeObject: (value: TransactionObjectInput) => T) {
	function object(value: TransactionObjectInput) {
		return makeObject(value);
	}

	object.system = () => object('0x5');
	object.clock = () => object('0x6');
	object.random = () => object('0x8');
	object.denyList = () => object('0x403');

	return object;
}

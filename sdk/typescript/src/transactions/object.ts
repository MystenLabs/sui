// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Transaction, TransactionObjectInput } from './Transaction.js';

export function createObjectMethods<T>(makeObject: (value: TransactionObjectInput) => T) {
	function object(value: TransactionObjectInput) {
		return makeObject(value);
	}

	object.system = () => object('0x5');
	object.clock = () => object('0x6');
	object.random = () => object('0x8');
	object.denyList = () => object('0x403');
	object.option =
		({ type, value }: { type: string; value: TransactionObjectInput | null }) =>
		(tx: Transaction) =>
			tx.moveCall({
				typeArguments: [type],
				target: `0x1::option::${value === null ? 'none' : 'some'}`,
				arguments: value === null ? [] : [tx.object(value)],
			});

	return object;
}

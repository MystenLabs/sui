// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Inputs } from './Inputs.js';
import { createPure } from './pure.js';
import type { Transaction, TransactionObjectInput } from './Transaction.js';

export const Arguments = {
	pure: createPure((value) => (tx: Transaction) => tx.pure(value)),
	object: (value: TransactionObjectInput) => (tx: Transaction) => tx.object(value),
	sharedObjectRef:
		(...args: Parameters<(typeof Inputs)['SharedObjectRef']>) =>
		(tx: Transaction) =>
			tx.sharedObjectRef(...args),
	objectRef:
		(...args: Parameters<(typeof Inputs)['ObjectRef']>) =>
		(tx: Transaction) =>
			tx.objectRef(...args),
	receivingRef:
		(...args: Parameters<(typeof Inputs)['ReceivingRef']>) =>
		(tx: Transaction) =>
			tx.receivingRef(...args),
};

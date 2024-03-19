// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SerializedBcs } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { TransactionBlockInput } from './Transactions.js';

export function createPure(
	makePure: (value: unknown, type?: string | undefined) => TransactionBlockInput,
) {
	/**
	 * Add a new non-object input to the transaction.
	 */
	function pure(
		/**
		 * The pure value, serialized to BCS. If this is a Uint8Array, then the value
		 * is assumed to be raw bytes, and will be used directly.
		 */
		value: SerializedBcs<any, any> | Uint8Array,
	): TransactionBlockInput;
	/**
	 * @deprecated Pass the pure value as SerializedBcs instead.
	 */
	function pure(
		/**
		 * The pure value that will be used as the input value. If this is a Uint8Array, then the value
		 * is assumed to be raw bytes, and will be used directly.
		 */
		value: unknown,
		/**
		 * The BCS type to serialize the value into. If not provided, the type will automatically be determined
		 * based on how the input is used.
		 */
		type?: string,
	): TransactionBlockInput;

	function pure(value: unknown, type?: string): TransactionBlockInput {
		return makePure(value, type);
	}

	pure.u8 = (value: number) => makePure(bcs.U8.serialize(value));
	pure.u16 = (value: number) => makePure(bcs.U16.serialize(value));
	pure.u32 = (value: number) => makePure(bcs.U32.serialize(value));
	pure.u64 = (value: bigint | number | string) => makePure(bcs.U64.serialize(value));
	pure.u128 = (value: bigint | number | string) => makePure(bcs.U128.serialize(value));
	pure.u256 = (value: bigint | number | string) => makePure(bcs.U256.serialize(value));
	pure.bool = (value: boolean) => makePure(bcs.Bool.serialize(value));
	pure.string = (value: string) => makePure(bcs.String.serialize(value));
	pure.address = (value: string) => makePure(bcs.Address.serialize(value));
	pure.id = pure.address;

	return pure;
}

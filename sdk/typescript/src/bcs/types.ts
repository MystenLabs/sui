// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type SharedObjectRef = {
	/** Hex code as string representing the object id */
	objectId: string;

	/** The version the object was shared at */
	initialSharedVersion: number | string;

	/** Whether reference is mutable */
	mutable: boolean;
};

export type SuiObjectRef = {
	/** Base64 string representing the object digest */
	objectId: string;
	/** Object version */
	version: number | string;
	/** Hex code as string representing the object id */
	digest: string;
};

/**
 * An object argument.
 */
export type ObjectArg =
	| { ImmOrOwnedObject: SuiObjectRef }
	| { SharedObject: SharedObjectRef }
	| { Receiving: SuiObjectRef };

export type ObjectCallArg = {
	Object: ObjectArg;
};

/**
 * A pure argument.
 */
export type PureArg = { Pure: Array<number> };

export function isPureArg(arg: any): arg is PureArg {
	return (arg as PureArg).Pure !== undefined;
}

/**
 * An argument for the transaction. It is a 'meant' enum which expects to have
 * one of the optional properties. If not, the BCS error will be thrown while
 * attempting to form a transaction.
 *
 * Example:
 * ```js
 * let arg1: CallArg = { Object: { Shared: {
 *   objectId: '5460cf92b5e3e7067aaace60d88324095fd22944',
 *   initialSharedVersion: 1,
 *   mutable: true,
 * } } };
 * let arg2: CallArg = { Pure: bcs.ser(BCS.STRING, 100000).toBytes() };
 * let arg3: CallArg = { Object: { ImmOrOwned: {
 *   objectId: '4047d2e25211d87922b6650233bd0503a6734279',
 *   version: 1,
 *   digest: 'bCiANCht4O9MEUhuYjdRCqRPZjr2rJ8MfqNiwyhmRgA='
 * } } };
 * ```
 *
 * For `Pure` arguments BCS is required. You must encode the values with BCS according
 * to the type required by the called function. Pure accepts only serialized values
 */
export type CallArg = PureArg | ObjectCallArg;

/**
 * Kind of a TypeTag which is represented by a Move type identifier.
 */
export type StructTag = {
	address: string;
	module: string;
	name: string;
	typeParams: TypeTag[];
};

/**
 * Sui TypeTag object. A decoupled `0x...::module::Type<???>` parameter.
 */
export type TypeTag =
	| { bool: null | true }
	| { u8: null | true }
	| { u64: null | true }
	| { u128: null | true }
	| { address: null | true }
	| { signer: null | true }
	| { vector: TypeTag }
	| { struct: StructTag }
	| { u16: null | true }
	| { u32: null | true }
	| { u256: null | true };

// ========== TransactionData ===========

/**
 * The GasData to be used in the transaction.
 */
export type GasData = {
	payment: SuiObjectRef[];
	owner: string; // Gas Object's owner
	price: number;
	budget: number;
};

/**
 * TransactionExpiration
 *
 * Indications the expiration time for a transaction.
 */
export type TransactionExpiration = { None: null } | { Epoch: number };

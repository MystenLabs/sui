// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsType, BcsTypeOptions } from '@mysten/bcs';
import { bcs, fromB58, fromB64, fromHEX, toB58, toB64, toHEX } from '@mysten/bcs';

import { normalizeSuiAddress, SUI_ADDRESS_LENGTH } from '../utils/sui-types.js';

export { TypeTagSerializer } from './type-tag-serializer.js';
export { BcsType, type BcsTypeOptions } from '@mysten/bcs';

/**
 * A reference to a shared object.
 */
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

function unsafe_u64(options?: BcsTypeOptions<number>) {
	return bcs
		.u64({
			name: 'unsafe_u64',
			...(options as object),
		})
		.transform({
			input: (val: number | string) => val,
			output: (val) => Number(val),
		});
}

function optionEnum<T extends BcsType<any, any>>(type: T) {
	return bcs.enum('Option', {
		None: null,
		Some: type,
	});
}

const Address = bcs.bytes(SUI_ADDRESS_LENGTH).transform({
	input: (val: string | Uint8Array) =>
		typeof val === 'string' ? fromHEX(normalizeSuiAddress(val)) : val,
	output: (val) => normalizeSuiAddress(toHEX(val)),
});

const ObjectDigest = bcs.vector(bcs.u8()).transform({
	name: 'ObjectDigest',
	input: (value: string) => fromB58(value),
	output: (value) => toB58(new Uint8Array(value)),
});

const SuiObjectRef = bcs.struct('SuiObjectRef', {
	objectId: Address,
	version: bcs.u64(),
	digest: ObjectDigest,
});

const SharedObjectRef = bcs.struct('SharedObjectRef', {
	objectId: Address,
	initialSharedVersion: bcs.u64(),
	mutable: bcs.bool(),
});

const ObjectArg = bcs.enum('ObjectArg', {
	ImmOrOwnedObject: SuiObjectRef,
	SharedObject: SharedObjectRef,
	Receiving: SuiObjectRef,
});

const CallArg = bcs.enum('CallArg', {
	Pure: bcs.vector(bcs.u8()),
	Object: ObjectArg,
	// ObjVec: bcs.vector(ObjectArg),
});

const TypeTag: BcsType<TypeTag> = bcs.enum('TypeTag', {
	bool: null,
	u8: null,
	u64: null,
	u128: null,
	address: null,
	signer: null,
	vector: bcs.lazy(() => TypeTag),
	struct: bcs.lazy(() => StructTag),
	u16: null,
	u32: null,
	u256: null,
}) as never;

const Argument = bcs.enum('Argument', {
	GasCoin: null,
	Input: bcs.u16(),
	Result: bcs.u16(),
	NestedResult: bcs.tuple([bcs.u16(), bcs.u16()]),
});

const ProgrammableMoveCall = bcs.struct('ProgrammableMoveCall', {
	package: Address,
	module: bcs.string(),
	function: bcs.string(),
	typeArguments: bcs.vector(TypeTag),
	arguments: bcs.vector(Argument),
});

const Transaction = bcs.enum('Transaction', {
	/**
	 * A Move Call - any public Move function can be called via
	 * this transaction. The results can be used that instant to pass
	 * into the next transaction.
	 */
	MoveCall: ProgrammableMoveCall,
	/**
	 * Transfer vector of objects to a receiver.
	 */
	TransferObjects: bcs.tuple([bcs.vector(Argument), Argument]),
	// /**
	//  * Split `amount` from a `coin`.
	//  */
	SplitCoins: bcs.tuple([Argument, bcs.vector(Argument)]),
	// /**
	//  * Merge Vector of Coins (`sources`) into a `destination`.
	//  */
	MergeCoins: bcs.tuple([Argument, bcs.vector(Argument)]),
	// /**
	//  * Publish a Move module.
	//  */
	Publish: bcs.tuple([bcs.vector(bcs.vector(bcs.u8())), bcs.vector(Address)]),
	// /**
	//  * Build a vector of objects using the input arguments.
	//  * It is impossible to construct a `vector<T: key>` otherwise,
	//  * so this call serves a utility function.
	//  */
	MakeMoveVec: bcs.tuple([optionEnum(TypeTag), bcs.vector(Argument)]),
	// /**  */
	Upgrade: bcs.tuple([bcs.vector(bcs.vector(bcs.u8())), bcs.vector(Address), Address, Argument]),
});

const ProgrammableTransaction = bcs.struct('ProgrammableTransaction', {
	inputs: bcs.vector(CallArg),
	transactions: bcs.vector(Transaction),
});

const TransactionKind = bcs.enum('TransactionKind', {
	ProgrammableTransaction: ProgrammableTransaction,
	ChangeEpoch: null,
	Genesis: null,
	ConsensusCommitPrologue: null,
});

const TransactionExpiration = bcs.enum('TransactionExpiration', {
	None: null,
	Epoch: unsafe_u64(),
});

const StructTag = bcs.struct('StructTag', {
	address: Address,
	module: bcs.string(),
	name: bcs.string(),
	typeParams: bcs.vector(TypeTag),
});

const GasData = bcs.struct('GasData', {
	payment: bcs.vector(SuiObjectRef),
	owner: Address,
	price: bcs.u64(),
	budget: bcs.u64(),
});

const TransactionDataV1 = bcs.struct('TransactionDataV1', {
	kind: TransactionKind,
	sender: Address,
	gasData: GasData,
	expiration: TransactionExpiration,
});

const TransactionData = bcs.enum('TransactionData', {
	V1: TransactionDataV1,
});

const IntentScope = bcs.enum('IntentScope', {
	TransactionData: null,
	TransactionEffects: null,
	CheckpointSummary: null,
	PersonalMessage: null,
});

const IntentVersion = bcs.enum('IntentVersion', {
	V0: null,
});

const AppId = bcs.enum('AppId', {
	Sui: null,
});

const Intent = bcs.struct('Intent', {
	scope: IntentScope,
	version: IntentVersion,
	appId: AppId,
});

function IntentMessage<T extends BcsType<any>>(T: T) {
	return bcs.struct(`IntentMessage<${T.name}>`, {
		intent: Intent,
		value: T,
	});
}

const CompressedSignature = bcs.enum('CompressedSignature', {
	ED25519: bcs.fixedArray(64, bcs.u8()),
	Secp256k1: bcs.fixedArray(64, bcs.u8()),
	Secp256r1: bcs.fixedArray(64, bcs.u8()),
	ZkLogin: bcs.vector(bcs.u8()),
});

const PublicKey = bcs.enum('PublicKey', {
	ED25519: bcs.fixedArray(32, bcs.u8()),
	Secp256k1: bcs.fixedArray(33, bcs.u8()),
	Secp256r1: bcs.fixedArray(33, bcs.u8()),
	ZkLogin: bcs.vector(bcs.u8()),
});

const MultiSigPkMap = bcs.struct('MultiSigPkMap', {
	pubKey: PublicKey,
	weight: bcs.u8(),
});

const MultiSigPublicKey = bcs.struct('MultiSigPublicKey', {
	pk_map: bcs.vector(MultiSigPkMap),
	threshold: bcs.u16(),
});

const MultiSig = bcs.struct('MultiSig', {
	sigs: bcs.vector(CompressedSignature),
	bitmap: bcs.u16(),
	multisig_pk: MultiSigPublicKey,
});

const base64String = bcs.vector(bcs.u8()).transform({
	input: (val: string | Uint8Array) => (typeof val === 'string' ? fromB64(val) : val),
	output: (val) => toB64(new Uint8Array(val)),
});

const SenderSignedTransaction = bcs.struct('SenderSignedTransaction', {
	intentMessage: IntentMessage(TransactionData),
	txSignatures: bcs.vector(base64String),
});

const SenderSignedData = bcs.vector(SenderSignedTransaction, {
	name: 'SenderSignedData',
});

const suiBcs = {
	...bcs,
	U8: bcs.u8(),
	U16: bcs.u16(),
	U32: bcs.u32(),
	U64: bcs.u64(),
	U128: bcs.u128(),
	U256: bcs.u256(),
	ULEB128: bcs.uleb128(),
	Bool: bcs.bool(),
	String: bcs.string(),
	Address,
	Argument,
	CallArg,
	CompressedSignature,
	GasData,
	MultiSig,
	MultiSigPkMap,
	MultiSigPublicKey,
	ObjectArg,
	ObjectDigest,
	ProgrammableMoveCall,
	ProgrammableTransaction,
	PublicKey,
	SenderSignedData,
	SenderSignedTransaction,
	SharedObjectRef,
	StructTag,
	SuiObjectRef,
	Transaction,
	TransactionData,
	TransactionDataV1,
	TransactionExpiration,
	TransactionKind,
	TypeTag,
};

export { suiBcs as bcs };

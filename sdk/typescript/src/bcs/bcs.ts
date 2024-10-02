// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsType, BcsTypeOptions } from '@mysten/bcs';
import { bcs, fromBase58, fromBase64, fromHex, toBase58, toBase64, toHex } from '@mysten/bcs';

import { isValidSuiAddress, normalizeSuiAddress, SUI_ADDRESS_LENGTH } from '../utils/sui-types.js';
import { TypeTagSerializer } from './type-tag-serializer.js';
import type { TypeTag as TypeTagType } from './types.js';

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

export const Address = bcs.bytes(SUI_ADDRESS_LENGTH).transform({
	validate: (val) => {
		const address = typeof val === 'string' ? val : toHex(val);
		if (!address || !isValidSuiAddress(normalizeSuiAddress(address))) {
			throw new Error(`Invalid Sui address ${address}`);
		}
	},
	input: (val: string | Uint8Array) =>
		typeof val === 'string' ? fromHex(normalizeSuiAddress(val)) : val,
	output: (val) => normalizeSuiAddress(toHex(val)),
});

export const ObjectDigest = bcs.vector(bcs.u8()).transform({
	name: 'ObjectDigest',
	input: (value: string) => fromBase58(value),
	output: (value) => toBase58(new Uint8Array(value)),
	validate: (value) => {
		if (fromBase58(value).length !== 32) {
			throw new Error('ObjectDigest must be 32 bytes');
		}
	},
});

export const SuiObjectRef = bcs.struct('SuiObjectRef', {
	objectId: Address,
	version: bcs.u64(),
	digest: ObjectDigest,
});

export const SharedObjectRef = bcs.struct('SharedObjectRef', {
	objectId: Address,
	initialSharedVersion: bcs.u64(),
	mutable: bcs.bool(),
});

export const ObjectArg = bcs.enum('ObjectArg', {
	ImmOrOwnedObject: SuiObjectRef,
	SharedObject: SharedObjectRef,
	Receiving: SuiObjectRef,
});

export const CallArg = bcs.enum('CallArg', {
	Pure: bcs.struct('Pure', {
		bytes: bcs.vector(bcs.u8()).transform({
			input: (val: string | Uint8Array) => (typeof val === 'string' ? fromBase64(val) : val),
			output: (val) => toBase64(new Uint8Array(val)),
		}),
	}),
	Object: ObjectArg,
});

const InnerTypeTag: BcsType<TypeTagType, TypeTagType> = bcs.enum('TypeTag', {
	bool: null,
	u8: null,
	u64: null,
	u128: null,
	address: null,
	signer: null,
	vector: bcs.lazy(() => InnerTypeTag),
	struct: bcs.lazy(() => StructTag),
	u16: null,
	u32: null,
	u256: null,
}) as BcsType<TypeTagType>;

export const TypeTag = InnerTypeTag.transform({
	input: (typeTag: string | TypeTagType) =>
		typeof typeTag === 'string' ? TypeTagSerializer.parseFromStr(typeTag, true) : typeTag,
	output: (typeTag: TypeTagType) => TypeTagSerializer.tagToString(typeTag),
});

export const Argument = bcs.enum('Argument', {
	GasCoin: null,
	Input: bcs.u16(),
	Result: bcs.u16(),
	NestedResult: bcs.tuple([bcs.u16(), bcs.u16()]),
});

export const ProgrammableMoveCall = bcs.struct('ProgrammableMoveCall', {
	package: Address,
	module: bcs.string(),
	function: bcs.string(),
	typeArguments: bcs.vector(TypeTag),
	arguments: bcs.vector(Argument),
});

export const Command = bcs.enum('Command', {
	/**
	 * A Move Call - any public Move function can be called via
	 * this transaction. The results can be used that instant to pass
	 * into the next transaction.
	 */
	MoveCall: ProgrammableMoveCall,
	/**
	 * Transfer vector of objects to a receiver.
	 */
	TransferObjects: bcs.struct('TransferObjects', {
		objects: bcs.vector(Argument),
		address: Argument,
	}),
	// /**
	//  * Split `amount` from a `coin`.
	//  */
	SplitCoins: bcs.struct('SplitCoins', {
		coin: Argument,
		amounts: bcs.vector(Argument),
	}),
	// /**
	//  * Merge Vector of Coins (`sources`) into a `destination`.
	//  */
	MergeCoins: bcs.struct('MergeCoins', {
		destination: Argument,
		sources: bcs.vector(Argument),
	}),
	// /**
	//  * Publish a Move module.
	//  */
	Publish: bcs.struct('Publish', {
		modules: bcs.vector(
			bcs.vector(bcs.u8()).transform({
				input: (val: string | Uint8Array) => (typeof val === 'string' ? fromBase64(val) : val),
				output: (val) => toBase64(new Uint8Array(val)),
			}),
		),
		dependencies: bcs.vector(Address),
	}),
	// /**
	//  * Build a vector of objects using the input arguments.
	//  * It is impossible to export construct a `vector<T: key>` otherwise,
	//  * so this call serves a utility function.
	//  */
	MakeMoveVec: bcs.struct('MakeMoveVec', {
		type: optionEnum(TypeTag).transform({
			input: (val: string | null) =>
				val === null
					? {
							None: true,
						}
					: {
							Some: val,
						},
			output: (val) => val.Some ?? null,
		}),
		elements: bcs.vector(Argument),
	}),
	Upgrade: bcs.struct('Upgrade', {
		modules: bcs.vector(
			bcs.vector(bcs.u8()).transform({
				input: (val: string | Uint8Array) => (typeof val === 'string' ? fromBase64(val) : val),
				output: (val) => toBase64(new Uint8Array(val)),
			}),
		),
		dependencies: bcs.vector(Address),
		package: Address,
		ticket: Argument,
	}),
});

export const ProgrammableTransaction = bcs.struct('ProgrammableTransaction', {
	inputs: bcs.vector(CallArg),
	commands: bcs.vector(Command),
});

export const TransactionKind = bcs.enum('TransactionKind', {
	ProgrammableTransaction: ProgrammableTransaction,
	ChangeEpoch: null,
	Genesis: null,
	ConsensusCommitPrologue: null,
});

export const TransactionExpiration = bcs.enum('TransactionExpiration', {
	None: null,
	Epoch: unsafe_u64(),
});

export const StructTag = bcs.struct('StructTag', {
	address: Address,
	module: bcs.string(),
	name: bcs.string(),
	typeParams: bcs.vector(InnerTypeTag),
});

export const GasData = bcs.struct('GasData', {
	payment: bcs.vector(SuiObjectRef),
	owner: Address,
	price: bcs.u64(),
	budget: bcs.u64(),
});

export const TransactionDataV1 = bcs.struct('TransactionDataV1', {
	kind: TransactionKind,
	sender: Address,
	gasData: GasData,
	expiration: TransactionExpiration,
});

export const TransactionData = bcs.enum('TransactionData', {
	V1: TransactionDataV1,
});

export const IntentScope = bcs.enum('IntentScope', {
	TransactionData: null,
	TransactionEffects: null,
	CheckpointSummary: null,
	PersonalMessage: null,
});

export const IntentVersion = bcs.enum('IntentVersion', {
	V0: null,
});

export const AppId = bcs.enum('AppId', {
	Sui: null,
});

export const Intent = bcs.struct('Intent', {
	scope: IntentScope,
	version: IntentVersion,
	appId: AppId,
});

export function IntentMessage<T extends BcsType<any>>(T: T) {
	return bcs.struct(`IntentMessage<${T.name}>`, {
		intent: Intent,
		value: T,
	});
}

export const CompressedSignature = bcs.enum('CompressedSignature', {
	ED25519: bcs.fixedArray(64, bcs.u8()),
	Secp256k1: bcs.fixedArray(64, bcs.u8()),
	Secp256r1: bcs.fixedArray(64, bcs.u8()),
	ZkLogin: bcs.vector(bcs.u8()),
});

export const PublicKey = bcs.enum('PublicKey', {
	ED25519: bcs.fixedArray(32, bcs.u8()),
	Secp256k1: bcs.fixedArray(33, bcs.u8()),
	Secp256r1: bcs.fixedArray(33, bcs.u8()),
	ZkLogin: bcs.vector(bcs.u8()),
});

export const MultiSigPkMap = bcs.struct('MultiSigPkMap', {
	pubKey: PublicKey,
	weight: bcs.u8(),
});

export const MultiSigPublicKey = bcs.struct('MultiSigPublicKey', {
	pk_map: bcs.vector(MultiSigPkMap),
	threshold: bcs.u16(),
});

export const MultiSig = bcs.struct('MultiSig', {
	sigs: bcs.vector(CompressedSignature),
	bitmap: bcs.u16(),
	multisig_pk: MultiSigPublicKey,
});

export const base64String = bcs.vector(bcs.u8()).transform({
	input: (val: string | Uint8Array) => (typeof val === 'string' ? fromBase64(val) : val),
	output: (val) => toBase64(new Uint8Array(val)),
});

export const SenderSignedTransaction = bcs.struct('SenderSignedTransaction', {
	intentMessage: IntentMessage(TransactionData),
	txSignatures: bcs.vector(base64String),
});

export const SenderSignedData = bcs.vector(SenderSignedTransaction, {
	name: 'SenderSignedData',
});

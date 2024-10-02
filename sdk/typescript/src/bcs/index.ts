// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/bcs';

import {
	Address,
	AppId,
	Argument,
	CallArg,
	Command,
	CompressedSignature,
	GasData,
	Intent,
	IntentMessage,
	IntentScope,
	IntentVersion,
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
	TransactionData,
	TransactionDataV1,
	TransactionExpiration,
	TransactionKind,
	TypeTag,
} from './bcs.js';
import { TransactionEffects } from './effects.js';

export type { TypeTag } from './types.js';

export { TypeTagSerializer } from './type-tag-serializer.js';
export { BcsType, type BcsTypeOptions } from '@mysten/bcs';

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
	AppId,
	Argument,
	CallArg,
	CompressedSignature,
	GasData,
	Intent,
	IntentMessage,
	IntentScope,
	IntentVersion,
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
	Command,
	TransactionData,
	TransactionDataV1,
	TransactionExpiration,
	TransactionKind,
	TypeTag,
	TransactionEffects,
};

export { suiBcs as bcs };

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TypeName } from '@mysten/bcs';

export const ARGUMENT_INNER = 'Argument';
export const VECTOR = 'vector';
export const OPTION = 'Option';
export const CALL_ARG = 'CallArg';
export const TYPE_TAG = 'TypeTag';
export const OBJECT_ARG = 'ObjectArg';
export const PROGRAMMABLE_TX_BLOCK = 'ProgrammableTransaction';
export const PROGRAMMABLE_CALL_INNER = 'ProgrammableMoveCall';
export const TRANSACTION_INNER = 'Transaction';
export const COMPRESSED_SIGNATURE = 'CompressedSignature';
export const PUBLIC_KEY = 'PublicKey';
export const MULTISIG_PUBLIC_KEY = 'MultiSigPublicKey';
export const MULTISIG_PK_MAP = 'MultiSigPkMap';
export const MULTISIG = 'MultiSig';

export const ENUM_KIND = 'EnumKind';

/** Wrapper around transaction Enum to support `kind` matching in TS */
export const TRANSACTION: TypeName = TRANSACTION_INNER;
/** Wrapper around Argument Enum to support `kind` matching in TS */
export const ARGUMENT: TypeName = ARGUMENT_INNER;

/** Custom serializer for decoding package, module, function easier */
export const PROGRAMMABLE_CALL = 'ProgrammableMoveCall';

/** Transaction types */

export type Option<T> = { some: T } | { none: true };

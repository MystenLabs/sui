// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { boolean, define, literal, nullable, object, record, string, union } from 'superstruct';
import type { CallArg } from '../bcs/index.js';

/** @deprecated Use `string` instead. */
export const TransactionDigest = string();
/** @deprecated Use `string` instead. */
export type TransactionDigest = Infer<typeof TransactionDigest>;

/** @deprecated Use `string` instead. */
export const TransactionEffectsDigest = string();
/** @deprecated Use `string` instead. */
export type TransactionEffectsDigest = Infer<typeof TransactionEffectsDigest>;

/** @deprecated Use `string` instead. */
export const TransactionEventDigest = string();
/** @deprecated Use `string` instead. */
export type TransactionEventDigest = Infer<typeof TransactionEventDigest>;

/** @deprecated Use `string` instead. */
export const ObjectId = string();
/** @deprecated Use `string` instead. */
export type ObjectId = Infer<typeof ObjectId>;

/** @deprecated Use `string` instead. */
export const SuiAddress = string();
/** @deprecated Use `string` instead. */
export type SuiAddress = Infer<typeof SuiAddress>;

/** @deprecated Use `string` instead. */
export const SequenceNumber = string();
/** @deprecated Use `string` instead. */
export type SequenceNumber = Infer<typeof SequenceNumber>;

export const ObjectOwner = union([
	object({
		AddressOwner: string(),
	}),
	object({
		ObjectOwner: string(),
	}),
	object({
		Shared: object({
			initial_shared_version: nullable(string()),
		}),
	}),
	literal('Immutable'),
]);
export type ObjectOwner = Infer<typeof ObjectOwner>;

export type SuiJsonValue = boolean | number | string | CallArg | Array<SuiJsonValue>;
export const SuiJsonValue = define<SuiJsonValue>('SuiJsonValue', () => true);

const ProtocolConfigValue = union([
	object({ u32: string() }),
	object({ u64: string() }),
	object({ f64: string() }),
]);
type ProtocolConfigValue = Infer<typeof ProtocolConfigValue>;

export const ProtocolConfig = object({
	attributes: record(string(), nullable(ProtocolConfigValue)),
	featureFlags: record(string(), boolean()),
	maxSupportedProtocolVersion: string(),
	minSupportedProtocolVersion: string(),
	protocolVersion: string(),
});
export type ProtocolConfig = Infer<typeof ProtocolConfig>;

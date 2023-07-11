// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import {
	boolean,
	define,
	literal,
	nullable,
	number,
	object,
	record,
	string,
	union,
} from 'superstruct';
import type { CallArg } from './sui-bcs.js';

export const TransactionDigest = string();
export type TransactionDigest = Infer<typeof TransactionDigest>;

export const TransactionEffectsDigest = string();
export type TransactionEffectsDigest = Infer<typeof TransactionEffectsDigest>;

export const TransactionEventDigest = string();
export type TransactionEventDigest = Infer<typeof TransactionEventDigest>;

export const ObjectId = string();
export type ObjectId = Infer<typeof ObjectId>;

export const SuiAddress = string();
export type SuiAddress = Infer<typeof SuiAddress>;

export const SequenceNumber = string();
export type SequenceNumber = Infer<typeof SequenceNumber>;

export const ObjectOwner = union([
	object({
		AddressOwner: SuiAddress,
	}),
	object({
		ObjectOwner: SuiAddress,
	}),
	object({
		Shared: object({
			initial_shared_version: number(),
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

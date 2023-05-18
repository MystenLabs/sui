// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  boolean,
  define,
  Infer,
  literal,
  nullable,
  number,
  object,
  record,
  string,
  union,
} from 'superstruct';
import { CallArg } from './sui-bcs';
import { fromB58 } from '@mysten/bcs';

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

export type SuiJsonValue =
  | boolean
  | number
  | string
  | CallArg
  | Array<SuiJsonValue>;
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

// source of truth is
// https://github.com/MystenLabs/sui/blob/acb2b97ae21f47600e05b0d28127d88d0725561d/crates/sui-types/src/base_types.rs#L171
const TX_DIGEST_LENGTH = 32;

/** Returns whether the tx digest is valid based on the serialization format */
export function isValidTransactionDigest(
  value: string,
): value is TransactionDigest {
  try {
    const buffer = fromB58(value);
    return buffer.length === TX_DIGEST_LENGTH;
  } catch (e) {
    return false;
  }
}

// TODO - can we automatically sync this with rust length definition?
// Source of truth is
// https://github.com/MystenLabs/sui/blob/acb2b97ae21f47600e05b0d28127d88d0725561d/crates/sui-types/src/base_types.rs#L67
// which uses the Move account address length
// https://github.com/move-language/move/blob/67ec40dc50c66c34fd73512fcc412f3b68d67235/language/move-core/types/src/account_address.rs#L23 .

export const SUI_ADDRESS_LENGTH = 32;
export function isValidSuiAddress(value: string): value is SuiAddress {
  return isHex(value) && getHexByteLength(value) === SUI_ADDRESS_LENGTH;
}

export function isValidSuiObjectId(value: string): boolean {
  return isValidSuiAddress(value);
}

type StructTag = {
  address: string;
  module: string;
  name: string;
  typeParams: (string | StructTag)[];
};

function parseTypeTag(type: string): string | StructTag {
  if (!type.includes('::')) return type;

  return parseStructTag(type);
}

export function parseStructTag(type: string): StructTag {
  const [address, module] = type.split('::');

  const rest = type.slice(address.length + module.length + 4);
  const name = rest.includes('<') ? rest.slice(0, rest.indexOf('<')) : rest;
  const typeParams = rest.includes('<')
    ? rest
        .slice(rest.indexOf('<') + 1, rest.lastIndexOf('>'))
        .split(',')
        .map((typeParam) => parseTypeTag(typeParam.trim()))
    : [];

  return {
    address: normalizeSuiAddress(address),
    module,
    name,
    typeParams,
  };
}

export function normalizeStructTag(type: string | StructTag): string {
  const { address, module, name, typeParams } =
    typeof type === 'string' ? parseStructTag(type) : type;

  const formattedTypeParams =
    typeParams.length > 0
      ? `<${typeParams
          .map((typeParam) =>
            typeof typeParam === 'string'
              ? typeParam
              : normalizeStructTag(typeParam),
          )
          .join(',')}>`
      : '';

  return `${address}::${module}::${name}${formattedTypeParams}`;
}

/**
 * Perform the following operations:
 * 1. Make the address lower case
 * 2. Prepend `0x` if the string does not start with `0x`.
 * 3. Add more zeros if the length of the address(excluding `0x`) is less than `SUI_ADDRESS_LENGTH`
 *
 * WARNING: if the address value itself starts with `0x`, e.g., `0x0x`, the default behavior
 * is to treat the first `0x` not as part of the address. The default behavior can be overridden by
 * setting `forceAdd0x` to true
 *
 */
export function normalizeSuiAddress(
  value: string,
  forceAdd0x: boolean = false,
): SuiAddress {
  let address = value.toLowerCase();
  if (!forceAdd0x && address.startsWith('0x')) {
    address = address.slice(2);
  }
  return `0x${address.padStart(SUI_ADDRESS_LENGTH * 2, '0')}`;
}

export function normalizeSuiObjectId(
  value: string,
  forceAdd0x: boolean = false,
): ObjectId {
  return normalizeSuiAddress(value, forceAdd0x);
}

function isHex(value: string): boolean {
  return /^(0x|0X)?[a-fA-F0-9]+$/.test(value) && value.length % 2 === 0;
}

function getHexByteLength(value: string): number {
  return /^(0x|0X)/.test(value) ? (value.length - 2) / 2 : value.length / 2;
}

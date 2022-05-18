// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** Base64 string representing the object digest */
export type TransactionDigest = string;
export type SuiAddress = string;
export type ObjectOwner =
  | { AddressOwner: SuiAddress }
  | { ObjectOwner: SuiAddress }
  | 'Shared'
  | 'Immutable';


const TX_DIGEST_BASE64_LENGTH = 44;
// taken from https://rgxdb.com/r/1NUN74O6
const VALID_BASE64_REGEX =
  /^(?:[a-zA-Z0-9+\/]{4})*(?:|(?:[a-zA-Z0-9+\/]{3}=)|(?:[a-zA-Z0-9+\/]{2}==)|(?:[a-zA-Z0-9+\/]{1}===))$/;

export function isValidTransactionDigest(value: string): value is TransactionDigest {
  return (new Base64DataBuffer(value)).getLength() === TX_DIGEST_LENGTH
    && VALID_BASE64_REGEX.test(value);
}

// TODO - can we automatically sync this with rust length definition?
const SUI_ADDRESS_LENGTH = 16;
export function isValidSuiAddress(value: string): value is SuiAddress {
  return isHex(value) &&
    getHexByteLength(value) === SUI_ADDRESS_LENGTH;
}

export function isValidSuiObjectId(value: string): boolean {
  return isValidSuiAddress(value);
}

function isHex(value: string): boolean {
  return /^(0x|0X)?[a-fA-F0-9]+$/.test(value) && value.length % 2 === 0;
}

function getHexByteLength(value: string): number {
  return /^(0x|0X)/.test(value)
    ? (value.length - 2) / 2
    : value.length / 2;
}
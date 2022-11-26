// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../serialization/base64';
import { ObjectId } from './objects';
import { bcs, TransactionData } from './sui-bcs';
import {
  PublicKey,
  PublicKeyInitData,
  SIGNATURE_SCHEME_TO_FLAG,
  SignatureScheme,
} from '../cryptography/publickey';
import { sha256Hash } from '../cryptography/hash';
import { Ed25519PublicKey } from '../cryptography/ed25519-publickey';
import { Secp256k1PublicKey } from '../cryptography/secp256k1-publickey';

/** Base64 string representing the object digest */
export type TransactionDigest = string;
export type SuiAddress = string;
export type ObjectOwner =
  | { AddressOwner: SuiAddress }
  | { ObjectOwner: SuiAddress }
  | { Shared: { initial_shared_version: number } }
  | 'Immutable';

// source of truth is
// https://github.com/MystenLabs/sui/blob/acb2b97ae21f47600e05b0d28127d88d0725561d/crates/sui-types/src/base_types.rs#L171
const TX_DIGEST_LENGTH = 32;
// taken from https://rgxdb.com/r/1NUN74O6
const VALID_BASE64_REGEX =
  /^(?:[a-zA-Z0-9+\/]{4})*(?:|(?:[a-zA-Z0-9+\/]{3}=)|(?:[a-zA-Z0-9+\/]{2}==)|(?:[a-zA-Z0-9+\/]{1}===))$/;

export function isValidTransactionDigest(
  value: string
): value is TransactionDigest {
  return (
    new Base64DataBuffer(value).getLength() === TX_DIGEST_LENGTH &&
    VALID_BASE64_REGEX.test(value)
  );
}

// TODO - can we automatically sync this with rust length definition?
// Source of truth is
// https://github.com/MystenLabs/sui/blob/acb2b97ae21f47600e05b0d28127d88d0725561d/crates/sui-types/src/base_types.rs#L67
// which uses the Move account address length
// https://github.com/move-language/move/blob/67ec40dc50c66c34fd73512fcc412f3b68d67235/language/move-core/types/src/account_address.rs#L23 .

export const SUI_ADDRESS_LENGTH = 20;
export function isValidSuiAddress(value: string): value is SuiAddress {
  return isHex(value) && getHexByteLength(value) === SUI_ADDRESS_LENGTH;
}

export function isValidSuiObjectId(value: string): boolean {
  return isValidSuiAddress(value);
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
  forceAdd0x: boolean = false
): SuiAddress {
  let address = value.toLowerCase();
  if (!forceAdd0x && address.startsWith('0x')) {
    address = address.slice(2);
  }
  return `0x${address.padStart(SUI_ADDRESS_LENGTH * 2, '0')}`;
}

export function normalizeSuiObjectId(
  value: string,
  forceAdd0x: boolean = false
): ObjectId {
  return normalizeSuiAddress(value, forceAdd0x);
}

/**
 * Generates transaction digest.
 *
 * @param data transaction data
 * @param signatureScheme signature scheme
 * @param signature signature as a base64 string
 * @param publicKey public key
 */
export function generateTransactionDigest(
  data: TransactionData,
  signatureScheme: SignatureScheme,
  signature: string,
  publicKey: PublicKeyInitData
): string {
  const signatureBytes = new Base64DataBuffer(signature).getData();

  let pk: PublicKey;
  switch (signatureScheme) {
    case 'ED25519':
      pk = new Ed25519PublicKey(publicKey);
      break;
    case 'Secp256k1':
      pk = new Secp256k1PublicKey(publicKey);
  }
  const publicKeyBytes = pk.toBytes();
  const schemeByte = new Uint8Array([SIGNATURE_SCHEME_TO_FLAG[signatureScheme]]);

  const txSignature = new Uint8Array(
    1 + signatureBytes.length + publicKeyBytes.length
  );
  txSignature.set(schemeByte);
  txSignature.set(signatureBytes, 1);
  txSignature.set(publicKeyBytes, 1 + signatureBytes.length);

  const senderSignedData = {
    data,
    txSignature,
  };
  const senderSignedDataBytes = bcs
    .ser('SenderSignedData', senderSignedData)
    .toBytes();

  return sha256Hash('SenderSignedData', senderSignedDataBytes);
}

function isHex(value: string): boolean {
  return /^(0x|0X)?[a-fA-F0-9]+$/.test(value) && value.length % 2 === 0;
}

function getHexByteLength(value: string): number {
  return /^(0x|0X)/.test(value) ? (value.length - 2) / 2 : value.length / 2;
}

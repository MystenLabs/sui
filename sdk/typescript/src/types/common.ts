// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base58DataBuffer } from '../serialization/base58';
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
import { Base64DataBuffer } from '../serialization/base64';

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

/** Returns whether the tx digest is valid based on the serialization format */
export function isValidTransactionDigest(
  value: string,
  serializationFmt: 'base64' | 'base58'
): value is TransactionDigest {
  let buffer;
  try {
    if (serializationFmt === 'base58') {
      buffer = new Base58DataBuffer(value);
    } else {
      buffer = new Base64DataBuffer(value);
    }
    return buffer.getLength() === TX_DIGEST_LENGTH;
  } catch (e) {
    return false;
  }
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
 * Generate transaction digest.
 *
 * @param data transaction data
 * @param signatureScheme signature scheme
 * @param signature signature as a base64 string
 * @param publicKey public key
 */
export function generateTransactionDigest(
  data: TransactionData,
  signatureScheme: SignatureScheme,
  signature: string | Base64DataBuffer,
  publicKey: PublicKeyInitData | PublicKey,
  serializationFmt: 'base64' | 'base58',
  excludeSig: boolean = false
): string {
  const signatureBytes = (
    typeof signature === 'string' ? new Base64DataBuffer(signature) : signature
  ).getData();

  let pk: PublicKey;
  switch (signatureScheme) {
    case 'ED25519':
      pk =
        publicKey instanceof Ed25519PublicKey
          ? publicKey
          : new Ed25519PublicKey(publicKey as PublicKeyInitData);
      break;
    case 'Secp256k1':
      pk =
        publicKey instanceof Secp256k1PublicKey
          ? publicKey
          : new Secp256k1PublicKey(publicKey as PublicKeyInitData);
  }
  const publicKeyBytes = pk.toBytes();
  const schemeByte = new Uint8Array([
    SIGNATURE_SCHEME_TO_FLAG[signatureScheme],
  ]);

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

  let hash;
  if (excludeSig) {
    const txBytes = bcs.ser('TransactionData', data).toBytes();
    hash = sha256Hash('TransactionData', txBytes);
  } else {
    hash = sha256Hash('SenderSignedData', senderSignedDataBytes);
  }

  return serializationFmt === 'base58'
    ? new Base58DataBuffer(hash).toString()
    : new Base64DataBuffer(hash).toString();
}

function isHex(value: string): boolean {
  return /^(0x|0X)?[a-fA-F0-9]+$/.test(value) && value.length % 2 === 0;
}

function getHexByteLength(value: string): number {
  return /^(0x|0X)/.test(value) ? (value.length - 2) / 2 : value.length / 2;
}

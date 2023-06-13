// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { Ed25519PublicKey } from './ed25519-publickey';
import { PublicKey } from './publickey';
import { Secp256k1PublicKey } from './secp256k1-publickey';
import { Secp256r1PublicKey } from './secp256r1-publickey';

/**
 * A keypair used for signing transactions.
 */
export type SignatureScheme =
  | 'ED25519'
  | 'Secp256k1'
  | 'Secp256r1'
  | 'MultiSig';

/**
 * Pair of signature and corresponding public key
 */
export type SignaturePubkeyPair = {
  signatureScheme: SignatureScheme;
  /** Base64-encoded signature */
  signature: Uint8Array;
  /** Base64-encoded public key */
  pubKey: PublicKey;
};

/**
 * (`flag || signature || pubkey` bytes, as base-64 encoded string).
 * Signature is committed to the intent message of the transaction data, as base-64 encoded string.
 */
export type SerializedSignature = string;

export const SIGNATURE_SCHEME_TO_FLAG = {
  ED25519: 0x00,
  Secp256k1: 0x01,
  Secp256r1: 0x02,
  MultiSig: 0x03,
};

export const SIGNATURE_FLAG_TO_SCHEME = {
  0x00: 'ED25519',
  0x01: 'Secp256k1',
  0x02: 'Secp256r1',
  0x03: 'MultiSig',
} as const;
export type SignatureFlag = keyof typeof SIGNATURE_FLAG_TO_SCHEME;

export function toSerializedSignature({
  signature,
  signatureScheme,
  pubKey,
}: SignaturePubkeyPair): SerializedSignature {
  const serializedSignature = new Uint8Array(
    1 + signature.length + pubKey.toBytes().length,
  );
  serializedSignature.set([SIGNATURE_SCHEME_TO_FLAG[signatureScheme]]);
  serializedSignature.set(signature, 1);
  serializedSignature.set(pubKey.toBytes(), 1 + signature.length);
  return toB64(serializedSignature);
}

export function fromSerializedSignature(
  serializedSignature: SerializedSignature,
): SignaturePubkeyPair {
  const bytes = fromB64(serializedSignature);
  const signatureScheme =
    SIGNATURE_FLAG_TO_SCHEME[bytes[0] as keyof typeof SIGNATURE_FLAG_TO_SCHEME];

  if (signatureScheme === 'MultiSig') {
    // TODO(joyqvq): add multisig parsing support
    throw new Error('MultiSig is not supported');
  }

  const SIGNATURE_SCHEME_TO_PUBLIC_KEY = {
    ED25519: Ed25519PublicKey,
    Secp256k1: Secp256k1PublicKey,
    Secp256r1: Secp256r1PublicKey,
  };

  const PublicKey = SIGNATURE_SCHEME_TO_PUBLIC_KEY[signatureScheme];

  const signature = bytes.slice(1, bytes.length - PublicKey.SIZE);
  const pubkeyBytes = bytes.slice(1 + signature.length);
  const pubKey = new PublicKey(pubkeyBytes);

  return {
    signatureScheme,
    signature,
    pubKey,
  };
}

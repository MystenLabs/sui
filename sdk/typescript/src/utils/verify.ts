// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import nacl from 'tweetnacl';
import { IntentScope, messageWithIntent } from './intent';
import { secp256k1 } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import {
  fromSerializedSignature,
  SerializedSignature,
} from '../cryptography/signature';
import { blake2b } from '@noble/hashes/blake2b';

// TODO: This might actually make sense to eventually move to the `Keypair` instances themselves, as
// it could allow the Sui.js to be tree-shaken a little better, possibly allowing keypairs that are
// not used (and their deps) to be entirely removed from the bundle.

/** Verify data that is signed with the expected scope. */
export async function verifyMessage(
  message: Uint8Array | string,
  serializedSignature: SerializedSignature,
  scope: IntentScope,
) {
  const signature = fromSerializedSignature(serializedSignature);
  const messageBytes = messageWithIntent(
    scope,
    typeof message === 'string' ? fromB64(message) : message,
  );
  const digest = blake2b(messageBytes, { dkLen: 32 });
  switch (signature.signatureScheme) {
    case 'ED25519':
      return nacl.sign.detached.verify(
        digest,
        signature.signature,
        signature.pubKey.toBytes(),
      );
    case 'Secp256k1':
      return secp256k1.verify(
        secp256k1.Signature.fromCompact(signature.signature),
        sha256(digest),
        signature.pubKey.toBytes(),
      );
    default:
      throw new Error(
        `Unknown signature scheme: "${signature.signatureScheme}"`,
      );
  }
}

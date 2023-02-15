// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import nacl from 'tweetnacl';
import { SerializedSignature } from '../signers/signer';
import { IntentScope, messageWithIntent } from './intent';
import * as secp from '@noble/secp256k1';
import { fromSerializedSignature } from '../cryptography/publickey';

// TODO: This might actually make sense to eventually move to the `Keypair` instances themselves, as
// it could allow the Sui.js to be tree-shaken a little better, possibly allowing keypairs that are
// not used (and their deps) to be entirely removed from the bundle.

/** Verify data that is signed with `signer.signMessage`. */
export async function verifyMessage(
  message: Uint8Array | string,
  serializedSignature: SerializedSignature,
) {
  const signature = fromSerializedSignature(serializedSignature);
  const messageBytes = messageWithIntent(
    IntentScope.PersonalMessage,
    typeof message === 'string' ? fromB64(message) : message,
  );

  switch (signature.signatureScheme) {
    case 'ED25519':
      return nacl.sign.detached.verify(
        messageBytes,
        fromB64(signature.signature),
        fromB64(signature.pubKey),
      );
    case 'Secp256k1':
      return secp.verify(
        secp.Signature.fromCompact(fromB64(signature.signature)),
        await secp.utils.sha256(messageBytes),
        fromB64(signature.pubKey),
      );
    default:
      throw new Error(
        `Unknown signature scheme: "${signature.signatureScheme}"`,
      );
  }
}

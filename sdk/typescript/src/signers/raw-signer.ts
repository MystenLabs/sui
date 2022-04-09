// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '../cryptography/ed25519-keypair';
import { Base64DataBuffer } from '../serialization/base64';
import { SignaturePubkeyPair, Signer } from './signer';

export class RawSigner extends Signer {
  private readonly _keypair: Ed25519Keypair;

  constructor(keypair: Ed25519Keypair) {
    super();
    this._keypair = keypair;
  }

  async getAddress(): Promise<string> {
    throw this._keypair.getPublicKey().toSuiAddress();
  }

  async signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
    return {
      signature: this._keypair.signData(data),
      pubKey: this._keypair.getPublicKey(),
    };
  }
}

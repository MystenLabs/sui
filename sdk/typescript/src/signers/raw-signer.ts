// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '../cryptography/ed25519-keypair';
import { Provider } from '../providers/provider';
import { Base64DataBuffer } from '../serialization/base64';
import { defineReadOnly } from '../utils/properties';
import { SignaturePubkeyPair, Signer } from './signer';

export class RawSigner extends Signer {
  private readonly _keypair: Ed25519Keypair;

  constructor(keypair: Ed25519Keypair, provider?: Provider) {
    super();
    this._keypair = keypair;
    defineReadOnly(this, 'provider', provider);
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

  connect(provider: Provider): Signer {
    return new RawSigner(this._keypair, provider);
  }
}

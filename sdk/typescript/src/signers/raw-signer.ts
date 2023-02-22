// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Keypair } from '../cryptography/keypair';
import {
  SerializedSignature,
  toSerializedSignature,
} from '../cryptography/signature';
import { Provider } from '../providers/provider';
import { SuiAddress } from '../types';
import { SignerWithProvider } from './signer-with-provider';
import { TxnDataSerializer } from './txn-data-serializers/txn-data-serializer';

export class RawSigner extends SignerWithProvider {
  private readonly keypair: Keypair;

  constructor(
    keypair: Keypair,
    provider?: Provider,
    serializer?: TxnDataSerializer,
  ) {
    super(provider, serializer);
    this.keypair = keypair;
  }

  async getAddress(): Promise<SuiAddress> {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  async signData(data: Uint8Array): Promise<SerializedSignature> {
    const pubkey = this.keypair.getPublicKey();
    const signature = this.keypair.signData(data, false);
    const signatureScheme = this.keypair.getKeyScheme();

    return toSerializedSignature({
      signatureScheme,
      signature,
      pubKey: pubkey,
    });
  }

  connect(provider: Provider): SignerWithProvider {
    return new RawSigner(this.keypair, provider);
  }
}

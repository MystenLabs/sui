// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { gt } from '@suchipi/femver';
import { Keypair } from '../cryptography/keypair';
import {
  SerializedSignature,
  toSerializedSignature,
} from '../cryptography/signature';
import { Provider } from '../providers/provider';
import { SuiAddress, versionToString } from '../types';
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
    // Starting Sui 0.25.0, only 64-byte nonrecoverable signatures are accepted.
    // TODO(joyqvq): Remove once 0.25.0 is released.
    const version = await this.provider.getRpcApiVersion();
    let useRecoverable =
      version && gt(versionToString(version), '0.24.0') ? false : true;

    const pubkey = this.keypair.getPublicKey();
    const signature = this.keypair.signData(data, useRecoverable);
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

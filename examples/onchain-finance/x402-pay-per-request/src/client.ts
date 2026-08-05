// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client-setup
import { Mppx } from 'mppx/client';
import { USDC, sui } from '@suimpp/mpp/client';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const signer = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);

const client = new SuiGrpcClient({
  network: 'mainnet',
  baseUrl: 'https://fullnode.mainnet.sui.io:443',
});

const mppx = Mppx.create({
  methods: [sui({ client, signer, currency: USDC })],
});
// docs::/#client-setup

// docs::#client-fetch
// Drop-in fetch replacement -- handles 402 challenges automatically
async function payForResource(url: string) {
  const response = await mppx.fetch(url);
  return response.json();
}
// docs::/#client-fetch

export { mppx, payForResource };

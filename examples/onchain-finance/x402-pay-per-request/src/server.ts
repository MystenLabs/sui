// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#server-setup
import { Mppx } from 'mppx';
import { InMemoryDigestStore, USDC, sui } from '@suimpp/mpp/server';

const mppx = Mppx.create({
  realm: 'api.example.com',
  methods: [sui({
    currency: USDC,
    recipient: process.env.PAY_TO_ADDRESS!,
    network: 'mainnet',
    store: new InMemoryDigestStore(), // Use Redis/Postgres in production
  })],
});
// docs::/#server-setup

// docs::#server-handler
// Wrap any handler -- mppx handles 402 challenges, verification, replay prevention
const handler = mppx.charge({ amount: '0.01' })(
  async () => Response.json({ result: 'paid content' })
);
// docs::/#server-handler

export { handler, mppx };

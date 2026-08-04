// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sponsor } from '@mysten-incubation/sponsor';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const sponsorKeypair = Ed25519Keypair.fromSecretKey(process.env.SPONSOR_SECRET_KEY!);
const allowedMoveCalls = new Set(
    (process.env.ALLOWED_MOVE_CALLS ?? '').split(',').map((value) => value.trim()).filter(Boolean),
);

// docs::#sponsor-sdk-create
const sponsor = new Sponsor({
    signer: sponsorKeypair,
    network: 'testnet',
});
// docs::/#sponsor-sdk-create

// docs::#sponsor-sdk-validate
// The package manages gas coins and the dual-signature flow. It does not decide
// what is safe to sponsor: that policy is yours, and it must deny by default.
const validatingSponsor = new Sponsor({
    signer: sponsorKeypair,
    network: 'testnet',
    validate: async (tx) => {
        const data = tx.getData();
        for (const command of data.commands) {
            if (command.$kind !== 'MoveCall') {
                throw new Error('Only Move calls are sponsored');
            }
            const { package: pkg, module, function: fn, arguments: args } = command.MoveCall;
            if (!allowedMoveCalls.has(`${pkg}::${module}::${fn}`)) {
                throw new Error('Move call is not allowed');
            }
            // Without this a caller can spend or transfer the sponsor gas coin.
            if (args.some((argument) => argument.$kind === 'GasCoin')) {
                throw new Error('Move calls cannot use the sponsor gas coin');
            }
        }
    },
});
// docs::/#sponsor-sdk-validate

// docs::#sponsor-sdk-sponsor
const { bytes, signature } = await sponsor.sponsor(txBytes);
// docs::/#sponsor-sdk-sponsor

declare const txBytes: Uint8Array;
export { sponsor, validatingSponsor, bytes, signature };

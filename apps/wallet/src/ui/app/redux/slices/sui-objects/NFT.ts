// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RawSigner, SuiExecuteTransactionResponse } from '@mysten/sui.js';

const DEFAULT_NFT_IMAGE =
    'ipfs://QmZPWWy5Si54R3d26toaqRiqvCH7HkGdXkxwUgCm2oKKM2?filename=img-sq-01.png';

// TODO: Remove this after internal dogfooding
export class ExampleNFT {
    /**
     * Mint a Example NFT. The wallet address must own enough gas tokens to pay for the transaction.
     *
     * @param signer A signer with connection to the fullnode
     */
    public static async mintExampleNFT(
        signer: RawSigner,
        name?: string,
        description?: string,
        imageUrl?: string
    ): Promise<SuiExecuteTransactionResponse> {
        return await signer.executeMoveCall({
            packageObjectId: '0x2',
            module: 'devnet_nft',
            function: 'mint',
            typeArguments: [],
            arguments: [
                name || 'Example NFT',
                description || 'An NFT created by Sui Wallet',
                imageUrl || DEFAULT_NFT_IMAGE,
            ],
            gasBudget: 10000,
        });
    }
}

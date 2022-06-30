// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionResponse, RawSigner } from '@mysten/sui.js';

// TODO: Remove this after internal dogfooding
export class ExampleNFT {
    /**
     * Mint a Example NFT. The wallet address must own enough gas tokens to pay for the transaction.
     *
     * @param signer A signer with connection to the gateway:e.g., new RawSigner(keypair, new JsonRpcProvider(endpoint))
     */
    public static async mintExampleNFT(
        signer: RawSigner,
        name?: string,
        description?: string,
        imageUrl?: string
    ): Promise<TransactionResponse> {
        return await signer.executeMoveCall({
            packageObjectId: '0x2',
            module: 'devnet_nft',
            function: 'mint',
            typeArguments: [],
            arguments: [
                name || 'Example NFT',
                description || 'An NFT created by Sui Wallet',
                imageUrl ||
                    'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
            ],
            gasBudget: 10000,
        });
    }

    // TODO marge this method with mintExampleNFT. Import type from @mysten/sui.js
    // transfer NFT to another address
    public static async TransferNFT(
        signer: RawSigner,
        nftId: string,
        recipientID: string
    ): Promise<TransactionResponse> {
        return await signer.executeMoveCall({
            packageObjectId: '0x2',
            module: 'devnet_nft',
            function: 'transfer',
            typeArguments: [],
            arguments: [nftId, recipientID],
            gasBudget: 10000,
        });
    }
}

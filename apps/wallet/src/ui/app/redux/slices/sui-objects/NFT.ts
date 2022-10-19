// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
    SuiTransactionResponse,
    RawSigner,
    SuiExecuteTransactionResponse,
} from '@mysten/sui.js';

const DEFAULT_NFT_IMAGE =
    'ipfs://QmZPWWy5Si54R3d26toaqRiqvCH7HkGdXkxwUgCm2oKKM2?filename=img-sq-01.png';

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
    ): Promise<SuiTransactionResponse> {
        await signer.syncAccountState();
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

    /**
     * Mint a Example NFT. The wallet address must own enough gas tokens to pay for the transaction.
     *
     * @param signer A signer with connection to the fullnode
     */
    public static async mintExampleNFTWithFullnode(
        signer: RawSigner,
        name?: string,
        description?: string,
        imageUrl?: string
    ): Promise<SuiExecuteTransactionResponse> {
        return await signer.executeMoveCallWithRequestType({
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

    // TODO marge this method with mintExampleNFT. Import type from @mysten/sui.js
    // transfer NFT to another address
    public static async TransferNFT(
        signer: RawSigner,
        nftId: string,
        recipientID: string,
        transferCost: number
    ): Promise<SuiTransactionResponse> {
        await signer.syncAccountState();
        return await signer.transferObject({
            objectId: nftId,
            gasBudget: transferCost,
            recipient: recipientID,
        });
    }

    public static async TransferNFTWithFullnode(
        signer: RawSigner,
        nftId: string,
        recipientID: string,
        transferCost: number
    ): Promise<SuiExecuteTransactionResponse> {
        return await signer.transferObjectWithRequestType({
            objectId: nftId,
            gasBudget: transferCost,
            recipient: recipientID,
        });
    }
}

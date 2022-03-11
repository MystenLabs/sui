// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Path, Controller, Get, Route } from 'tsoa';
import { NFTService, NFTGetResponse } from './nftService';

@Route('nfts/ethereum')
export class NFTController extends Controller {
    /**
     * Retrieves all the NFTs owned by the `source_chain_owner_address` and their airdrop
     * claim status on Sui.
     *
     * @param source_chain_owner_address The claimer's wallet address
     * @example source_chain_owner_address "0x95346AeB9979ec02b67904e6c3EbF7B7A0AC126D"
     */
    @Get('{source_chain_owner_address}')
    public async get(
        @Path() source_chain_owner_address: string
    ): Promise<NFTGetResponse> {
        return new NFTService().get(source_chain_owner_address);
    }
}

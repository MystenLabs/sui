// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    AlchemyMethods,
    createAlchemyWeb3,
    Nft as AlchemyNft,
} from '@alch/alchemy-web3';

interface NFT {
    /**
     * A descriptive name for the NFT
     */
    name: string;

    /**
     * The address of the collection contract
     */
    contract_address: string;

    /**
     * The token id associated with the source contract address
     */
    token_id: string;

    /**
     *  Uri representing the location of the NFT media asset. The uri often
     *  links to an image. The uri is parsed from the metadata and can be
     *  standard URLs pointing to images on conventional servers, IPFS, or
     *  Arweave. The image format can be SVGs, PNGs, JPEGs, etc.
     */
    media_uri?: string;
}

interface NFTInfo {
    token: NFT;
    claim_status: 'none' | 'claimed';
    destination_sui_address?: string;
    sui_explorer_link?: string;
}

/**
 *  NFTs owned by the address
 *
 * @example {
 *  "results":  [
 *    {
 *      "token": {
 *        "name": "BoredApeYachtClub #8221",
 *        "contract_address": "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D",
 *        "token_id": "8221",
 *        "media_uri": "ipfs://QmX96WhLEBJAuMNoWQatCmTesApsrNv74jDVNfzNaBbRoX"
 *      },
 *      "claim_status": "none"
 *    },
 *    {
 *      "token": {
 *        "name": "Azuki #124",
 *        "contract_address": "0xED5AF388653567Af2F388E6224dC7C4b3241C544",
 *        "token_id": "124",
 *        "media_uri": "https://ikzttp.mypinata.cloud/ipfs/QmQFkLSQysj94s5GvTHPyzTxrawwtjgiiYS2TBLgrvw8CW/124"
 *      },
 *      "claim_status": "claimed",
 *      "destination_sui_address": "0x10",
 *      "sui_explorer_link": "http:127.0.0.1:8000/0x1000"
 *    }
 *  ]
 * }
 */
export interface NFTGetResponse {
    /**
     * Metadata and claim status of the NFTs owned by the address
     */
    results: NFTInfo[];
    // TODO: implement pagination
}

export class NFTService {
    public async get(
        source_chain_owner_address: string
    ): Promise<NFTGetResponse> {
        const nftInfo = await this.getNFTInfo(source_chain_owner_address);
        return {
            results: nftInfo,
        };
    }

    private async getNFTInfo(address: string): Promise<NFTInfo[]> {
        const nfts = await this.getNFTsByAddress(address);
        return nfts.map((token) => ({
            token,
            // TODO: check db to see if airdrop has been claimed or not
            claim_status: 'none',
        }));
    }

    private async getNFTsByAddress(address: string): Promise<NFT[]> {
        const alchemy = this.getAlchemyAPI();
        const nfts = await alchemy.getNfts({ owner: address });
        console.log(nfts.totalCount);
        return nfts.ownedNfts.map((a) =>
            this.extractFieldsFromAlchemyNFT(a as AlchemyNft)
        );
    }

    private extractFieldsFromAlchemyNFT(alchemyNft: AlchemyNft): NFT {
        // TODO: look into using gateway uri https://docs.alchemy.com/alchemy/guides/nft-api-faq#understanding-nft-metadata
        const {
            title: name,
            metadata,
            id: { tokenId: token_id },
            contract: { address: contract_address },
        } = alchemyNft;
        return {
            contract_address,
            name,
            token_id,
            media_uri: metadata?.image,
        };
    }

    private getAlchemyAPI(): AlchemyMethods {
        // TODO: implement pagination
        const api_key = process.env.ALCHEMY_API_KEY || 'demo';
        return createAlchemyWeb3(
            `https://eth-mainnet.alchemyapi.io/v2/${api_key}`
        ).alchemy;
    }
}

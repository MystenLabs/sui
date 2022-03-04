// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

interface NFT {
    /**
     * A descriptive name for a collection of NFTs in this contract.
     */
    name: string;
    /**
     * An abbreviated name for NFTs in the contract.
     */
    symbol: string;
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
    media_uri: string;
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
 *        "name": "BoredApeYachtClub",
 *        "symbol": "BAYC",
 *        "token_id": "8221",
 *        "media_uri": "ipfs://QmX96WhLEBJAuMNoWQatCmTesApsrNv74jDVNfzNaBbRoX"
 *      },
 *      "claim_status": "none"
 *    },
 *    {
 *      "token": {
 *        "name": "Azuki",
 *        "symbol": "AZUKI",
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
    public get(source_chain_owner_address: string): NFTGetResponse {
        console.log(`fetching nfts for address ${source_chain_owner_address}`);
        return {
            results: [
                {
                    token: {
                        name: 'BoredApeYachtClub',
                        symbol: 'BAYC',
                        token_id: '8221',
                        media_uri:
                            'ipfs://QmX96WhLEBJAuMNoWQatCmTesApsrNv74jDVNfzNaBbRoX',
                    },
                    claim_status: 'none',
                },
                {
                    token: {
                        name: 'Azuki',
                        symbol: 'AZUKI',
                        token_id: '124',
                        media_uri:
                            'https://ikzttp.mypinata.cloud/ipfs/QmQFkLSQysj94s5GvTHPyzTxrawwtjgiiYS2TBLgrvw8CW/124',
                    },
                    claim_status: 'claimed',
                    destination_sui_address: '0x10',
                    sui_explorer_link: 'http:127.0.0.1:8000/0x1000',
                },
            ],
        };
    }
}

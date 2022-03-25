// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { NFTFetcher, NFTInfo } from '../common/nftFetcher';

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
        const fetcher = new NFTFetcher();
        const nftInfo = await fetcher.getNFTInfoByAddress(
            source_chain_owner_address
        );
        return {
            results: nftInfo,
        };
    }
}

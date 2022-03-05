// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

interface AirdropClaimMessage {
    /**
     * Name of the source chain
     * @pattern ethereum
     */
    source_chain: string;
    /**
     * The Contract address for the original NFT on the source chain
     */
    source_contract_address: string;
    /**
     * The token id for the original NFT on the source chain
     */
    source_token_id: string;

    /**
     * The address of the claimer's wallet on the source chain
     */
    source_owner_address: string;
    /**
     * The recipient of the NFT on Sui
     */
    destination_sui_address: string;
}

/**
 *  The params for an Airdrop claim request.
 *
 *
 * @example {
 *  "message": {
 *      "source_chain": "ethereum",
 *      "source_contract_address": "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D",
 *      "source_token_id": "101",
 *      "source_owner_address": "0x529f501ceb3ab599274a38f2aee41a7eba1fcead",
 *      "destination_sui_address": "0x10"
 *   },
 *   "signature": "0x21fbf0696d5e0aa2ef41a2b4ffb623bcaf070461d61cf7251c74161f82fec3a4370854bc0a34b3ab487c1bc021cd318c734c51ae29374f2beb0e6f2dd49b4bf41c"
 * }
 */
export interface AirdropClaimRequest {
    /**
     * unsigned message
     */
    message: AirdropClaimMessage;
    /**
     * Digital signature of `message` signed by the private key of `source_owner_address`
     */
    signature: string;
}

/**
 *  The response for an Airdrop claim request.
 *
 *
 * @example {
 *  "source_chain": "ethereum",
 *  "source_contract_address": "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D",
 *  "source_token_id": "101",
 *  "sui_explorer_link": "http:127.0.0.1:8000/BC4CA0EdA7647A8a"
 * }
 */
export interface AirdropClaimResponse {
    /**
     * Name of the source chain
     * @pattern ethereum
     */
    source_chain: string;
    /**
     * The Contract address for the original NFT on the source chain
     */
    source_contract_address: string;
    /**
     * The token id for the original NFT on the source chain
     */
    source_token_id: string;
    /**
     * The Sui Explorer Link to the newly minted airdrop NFT
     */
    sui_explorer_link: string;
}

export class AirdropService {
    public async claim(
        claimMessage: AirdropClaimRequest
    ): Promise<AirdropClaimResponse> {
        const message = claimMessage.message;
        return {
            source_chain: message.source_chain,
            source_contract_address: message.source_contract_address,
            source_token_id: message.source_token_id,
            sui_explorer_link: 'www.mystenlabs.com',
        };
    }
}

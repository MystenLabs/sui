// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { NFTFetcher, NFTInfo } from '../common/nftFetcher';
import { ValidateError } from 'tsoa';
import { Connection } from '../sdk/gateway';

const DEFAULT_GAS_BUDGET = 2000;

interface AirdropClaimInfo {
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
 *   "wallet_message": "{\"domain\":{\"chainId\":1,\"name\":\"SuiDrop\",\"version\":\"1\"},\"message\":{\"source_chain\":\"ethereum\",\"source_contract_address\":\"0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D\",\"source_token_id\":\"0x00000000000000000000000000000000000000000000000000000000000022e9\",\"source_owner_address\":\"0x09dbc4a902199bbe7f7ec29b3714731786f2e878\",\"destination_sui_address\":\"0xa5e6dbcf33730ace6ec8b400ff4788c1f150ff7e\"},\"primaryType\":\"ClaimRequest\",\"types\":{\"EIP712Domain\":[{\"name\":\"name\",\"type\":\"string\"},{\"name\":\"version\",\"type\":\"string\"},{\"name\":\"chainId\",\"type\":\"uint256\"}],\"ClaimRequest\":[{\"name\":\"source_chain\",\"type\":\"string\"},{\"name\":\"source_contract_address\",\"type\":\"string\"},{\"name\":\"source_token_id\",\"type\":\"string\"},{\"name\":\"source_owner_address\",\"type\":\"string\"},{\"name\":\"destination_sui_address\",\"type\":\"string\"}]}}",
 *   "signature": "abc"
 * }
 */
export interface AirdropClaimRequest {
    /**
     * An EIP-712 compliant message
     */
    wallet_message: string;
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
 *  "source_token_id": "0x00000000000000000000000000000000000000000000000000000000000022e9",
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
        const { wallet_message } = claimMessage;
        const data = JSON.parse(wallet_message);
        const claimInfo = this.parseClaimInfo(data);
        const nftInfo = await this.validateRequest(claimInfo);
        const connection = new Connection(
            process.env.SUI_GATEWAY_ENDPOINT as string
        );
        await this.executeMoveCall(connection, claimInfo, nftInfo);
        return {
            source_chain: claimInfo.source_chain,
            source_contract_address: claimInfo.source_contract_address,
            source_token_id: claimInfo.source_token_id,
            sui_explorer_link:
                'https://djgd7fpxio1yh.cloudfront.net/objects/7bc832ec31709638cd8d9323e90edf332gff4389',
        };
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    parseClaimInfo(data: any): AirdropClaimInfo {
        if (isAirdropClaimInfo(data['message'])) {
            return data['message'] as AirdropClaimInfo;
        }
        throw new ValidateError(
            { messages: { message: 'Wrong format', value: data['message'] } },
            'Wrong format for wallet message'
        );
    }

    async validateRequest(claimInfo: AirdropClaimInfo): Promise<NFTInfo> {
        const {
            source_contract_address: contract,
            source_token_id: tokenId,
            source_owner_address: owner,
        } = claimInfo;

        const results = await this.getNFTInfo(owner, contract, tokenId);

        if (results.length > 1) {
            throw new Error(
                `More than two tokens share the same contract ` +
                    `address and token id ${results}`
            );
        } else if (results.length === 0) {
            throw new ValidateError(
                {
                    messages: {
                        message: 'ownership not found',
                        value: claimInfo,
                    },
                },
                ''
            );
        }
        const nftInfo = results[0];

        if (nftInfo.claim_status !== 'none') {
            throw new ValidateError(
                {
                    messages: {
                        message: 'The token has been claimed',
                        value: claimInfo,
                    },
                },
                ''
            );
        }

        // TODO: validate signature
        return nftInfo;
    }

    async executeMoveCall(
        connection: Connection,
        claimInfo: AirdropClaimInfo,
        nftInfo: NFTInfo
    ): Promise<string> {
        const oracleAddress = process.env.ORACLE_ADDRESS as string;
        const [gasObjectId, oracleObjectId] = await this.getGasAndOracle(
            connection,
            oracleAddress
        );
        const {
            destination_sui_address,
            source_contract_address,
            source_token_id: tokenIdHex,
        } = claimInfo;

        const tokenId = parseInt(tokenIdHex, 16);
        console.log('token id', tokenId);
        const { name, media_uri } = nftInfo.token;
        const args = [
            oracleObjectId,
            destination_sui_address,
            source_contract_address,
            tokenId,
            name,
            media_uri,
        ];

        const [packageObjectId, module] = this.getPackageAndModule();

        const request = {
            args,
            function: process.env.ORACLE_CONTRACT_ENTRY_FUNCTION as string,
            gasBudget: DEFAULT_GAS_BUDGET,
            gasObjectId,
            module,
            packageObjectId,
            sender: oracleAddress,
        };
        const result = await connection.callMoveFunction(request);
        const created = result.objectEffectsSummary.created_objects;
        if (created.length !== 1) {
            throw new Error(`Unexpected number of objects created: ${created}`);
        }
        console.info('Created object', created);
        return created[0].id;
    }

    async getNFTInfo(
        owner: string,
        contract: string,
        tokenId: string
    ): Promise<NFTInfo[]> {
        const fetcher = new NFTFetcher();
        const results = await fetcher.getNFTInfo({
            owner,
            contractAddresses: [contract],
        });
        return results.filter((info) => info.token.token_id === tokenId);
    }

    async getGasAndOracle(
        connection: Connection,
        oracleAddress: string
    ): Promise<[string, string]> {
        const objects = await connection.bulkFetchObjects(oracleAddress);
        const gasCoin = objects.filter(
            (o) => o.objType === '0x2::Coin::Coin<0x2::GAS::GAS>'
        )[0].id;
        const oracle_object_identifier =
            this.getPackageAndModule().join('::') +
            '::' +
            (process.env.ORACLE_CONTRACT_ADMIN_IDENTIFIER as string);
        const oracle = objects.filter(
            (o) => o.objType === oracle_object_identifier
        );
        if (oracle.length !== 1) {
            throw new Error(`Unexpected number of oracle object: ${oracle}`);
        }

        return [
            this.formatObjectId(gasCoin),
            this.formatObjectId(oracle[0].id),
        ];
    }

    formatObjectId(id: string): string {
        return `0x${id}`;
    }

    getPackageAndModule(): [string, string] {
        const package_name = process.env.ORACLE_CONTRACT_PACKAGE || '0x2';
        const module_name =
            process.env.ORACLE_CONTRACT_MODULE || 'CrossChainAirdrop';
        return [package_name, module_name];
    }
}

/**
 * User Defined Type Guard for AirdropClaimInfo
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function isAirdropClaimInfo(arg: any): arg is AirdropClaimInfo {
    return (
        arg.source_chain &&
        arg.source_contract_address &&
        arg.source_token_id &&
        arg.source_owner_address &&
        arg.destination_sui_address
    );
}

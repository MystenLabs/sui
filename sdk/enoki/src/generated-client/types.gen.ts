// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type Nonce_Response = {
    data: {
        nonce: string;
        randomness: string;
        epoch: number;
        maxEpoch: number;
        estimatedExpiration: number;
    };
};

export type Nonce_Request = {
    /**
     * The Sui network you wish to use. Defaults to `mainnet`.
     */
    network?: 'testnet' | 'mainnet' | 'devnet';
    /**
     * The ephemeral public key created during the zkLogin process, encoded as a base64 string.
     */
    ephemeralPublicKey: string;
    /**
     * The amount of epochs that you would like to have the nonce be valid for.
     */
    additionalEpochs?: number;
};

/**
 * The Sui network you wish to use. Defaults to `mainnet`.
 */
export type network = 'testnet' | 'mainnet' | 'devnet';

export type ZKP_Response = {
    data: {
        proofPoints?: unknown;
        issBase64Details?: unknown;
        headerBase64?: unknown;
        addressSeed: string;
    };
};

export type ZKP_Request = {
    /**
     * The Sui network you wish to use. Defaults to `mainnet`.
     */
    network?: 'testnet' | 'mainnet' | 'devnet';
    /**
     * The ephemeral public key created during the zkLogin process, encoded as a base64 string.
     */
    ephemeralPublicKey: string;
    /**
     * The `maxEpoch` created during the zkLogin process.
     */
    maxEpoch: number;
    /**
     * The `randomness` created during the zkLogin process.
     */
    randomness: string;
};

export type CreateSponsoredTransactionResponse = {
    data: {
        digest: string;
        bytes: string;
    };
};

export type CreateSponsoredTransactionRequest = {
    /**
     * The Sui network you wish to use. Defaults to `mainnet`.
     */
    network?: 'testnet' | 'mainnet' | 'devnet';
    /**
     * Bytes of the transaction with the `onlyTransactionKind` flag set to true.
     */
    transactionBlockKindBytes: string;
    /**
     * The address sending the transaction. Include this parameter if not including the `zklogin-jwt` header. This option is only supported when calling the API from a backend service using a private key.
     */
    sender?: string;
    /**
     * List of Sui addresses that can be present in the transaction. These addresses are combined with the list configured in the Enoki Developer Portal. Transactions attempting to refer to or transfer assets outside of these addresses are rejected.
     */
    allowedAddresses?: Array<(string)>;
    /**
     * List of permitted Move targets the sponsored user's transactions can call.
     */
    allowedMoveCallTargets?: Array<(string)>;
};

export type ExecuteSponsoredTransactionResponse = {
    data: {
        digest: string;
    };
};

export type ExecuteSponsoredTransactionRequest = {
    /**
     * User signature of the transaction.
     */
    signature: string;
};

export type ZkLogin_Response = {
    data: {
        salt: string;
        address: string;
    };
};

export type App_Response = {
    data: {
        allowedOrigins: Array<(string)>;
        authenticationProviders: Array<{
            providerType: 'google' | 'facebook' | 'twitch' | 'apple';
            clientId: string | null;
        }>;
    };
};

export type PostV1ZkloginNonceData = {
    body?: Nonce_Request;
};

export type PostV1ZkloginNonceResponse = Nonce_Response;

export type PostV1ZkloginNonceError = unknown;

export type PostV1ZkloginZkpData = {
    body?: ZKP_Request;
    headers: {
        'zklogin-jwt': string;
    };
};

export type PostV1ZkloginZkpResponse = ZKP_Response;

export type PostV1ZkloginZkpError = unknown;

export type PostV1TransactionBlocksSponsorData = {
    body?: CreateSponsoredTransactionRequest;
    headers?: {
        'zklogin-jwt'?: string;
    };
};

export type PostV1TransactionBlocksSponsorResponse = CreateSponsoredTransactionResponse;

export type PostV1TransactionBlocksSponsorError = unknown;

export type PostV1TransactionBlocksSponsorByDigestData = {
    body?: ExecuteSponsoredTransactionRequest;
    path: {
        /**
         * The digest of the previously-created sponsored transaction block to execute.
         */
        digest: string;
    };
};

export type PostV1TransactionBlocksSponsorByDigestResponse = ExecuteSponsoredTransactionResponse;

export type PostV1TransactionBlocksSponsorByDigestError = unknown;

export type GetV1ZkloginData = {
    headers: {
        'zklogin-jwt': string;
    };
};

export type GetV1ZkloginResponse = ZkLogin_Response;

export type GetV1ZkloginError = unknown;

export type GetV1AppResponse = App_Response;

export type GetV1AppError = unknown;

export type $OpenApiTs = {
    '/v1/zklogin/nonce': {
        post: {
            req: PostV1ZkloginNonceData;
            res: {
                /**
                 * Successful response
                 */
                '200': Nonce_Response;
            };
        };
    };
    '/v1/zklogin/zkp': {
        post: {
            req: PostV1ZkloginZkpData;
            res: {
                /**
                 * Successful response
                 */
                '200': ZKP_Response;
            };
        };
    };
    '/v1/transaction-blocks/sponsor': {
        post: {
            req: PostV1TransactionBlocksSponsorData;
            res: {
                /**
                 * Successful response
                 */
                '200': CreateSponsoredTransactionResponse;
            };
        };
    };
    '/v1/transaction-blocks/sponsor/{digest}': {
        post: {
            req: PostV1TransactionBlocksSponsorByDigestData;
            res: {
                /**
                 * Successful response
                 */
                '200': ExecuteSponsoredTransactionResponse;
            };
        };
    };
    '/v1/zklogin': {
        get: {
            req: GetV1ZkloginData;
            res: {
                /**
                 * Successful response
                 */
                '200': ZkLogin_Response;
            };
        };
    };
    '/v1/app': {
        get: {
            res: {
                /**
                 * Successful response
                 */
                '200': App_Response;
            };
        };
    };
};
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export const $Nonce_Response = {
    type: 'object',
    properties: {
        data: {
            type: 'object',
            properties: {
                nonce: {
                    type: 'string'
                },
                randomness: {
                    type: 'string'
                },
                epoch: {
                    type: 'number'
                },
                maxEpoch: {
                    type: 'number'
                },
                estimatedExpiration: {
                    type: 'number'
                }
            },
            required: ['nonce', 'randomness', 'epoch', 'maxEpoch', 'estimatedExpiration']
        }
    },
    required: ['data']
} as const;

export const $Nonce_Request = {
    type: 'object',
    properties: {
        network: {
            type: 'string',
            enum: ['testnet', 'mainnet', 'devnet'],
            default: 'mainnet',
            description: 'The Sui network you wish to use. Defaults to `mainnet`.'
        },
        ephemeralPublicKey: {
            type: 'string',
            description: 'The ephemeral public key created during the zkLogin process, encoded as a base64 string.'
        },
        additionalEpochs: {
            type: 'number',
            maximum: 30,
            minimum: 0,
            default: 2,
            description: 'The amount of epochs that you would like to have the nonce be valid for.'
        }
    },
    required: ['ephemeralPublicKey']
} as const;

export const $ZKP_Response = {
    type: 'object',
    properties: {
        data: {
            type: 'object',
            properties: {
                proofPoints: {
                    nullable: true
                },
                issBase64Details: {
                    nullable: true
                },
                headerBase64: {
                    nullable: true
                },
                addressSeed: {
                    type: 'string'
                }
            },
            required: ['addressSeed']
        }
    },
    required: ['data']
} as const;

export const $ZKP_Request = {
    type: 'object',
    properties: {
        network: {
            type: 'string',
            enum: ['testnet', 'mainnet', 'devnet'],
            default: 'mainnet',
            description: 'The Sui network you wish to use. Defaults to `mainnet`.'
        },
        ephemeralPublicKey: {
            type: 'string',
            description: 'The ephemeral public key created during the zkLogin process, encoded as a base64 string.'
        },
        maxEpoch: {
            type: 'integer',
            minimum: 0,
            description: 'The `maxEpoch` created during the zkLogin process.'
        },
        randomness: {
            type: 'string',
            description: 'The `randomness` created during the zkLogin process.'
        }
    },
    required: ['ephemeralPublicKey', 'maxEpoch', 'randomness']
} as const;

export const $CreateSponsoredTransactionResponse = {
    type: 'object',
    properties: {
        data: {
            type: 'object',
            properties: {
                digest: {
                    type: 'string'
                },
                bytes: {
                    type: 'string'
                }
            },
            required: ['digest', 'bytes']
        }
    },
    required: ['data']
} as const;

export const $CreateSponsoredTransactionRequest = {
    type: 'object',
    properties: {
        network: {
            type: 'string',
            enum: ['testnet', 'mainnet', 'devnet'],
            default: 'mainnet',
            description: 'The Sui network you wish to use. Defaults to `mainnet`.'
        },
        transactionBlockKindBytes: {
            type: 'string',
            description: 'Bytes of the transaction with the `onlyTransactionKind` flag set to true.'
        },
        sender: {
            type: 'string',
            description: 'The address sending the transaction. Include this parameter if not including the `zklogin-jwt` header. This option is only supported when calling the API from a backend service using a private key.'
        },
        allowedAddresses: {
            type: 'array',
            items: {
                type: 'string'
            },
            description: 'List of Sui addresses that can be present in the transaction. These addresses are combined with the list configured in the Enoki Developer Portal. Transactions attempting to refer to or transfer assets outside of these addresses are rejected.'
        },
        allowedMoveCallTargets: {
            type: 'array',
            items: {
                type: 'string'
            },
            description: "List of permitted Move targets the sponsored user's transactions can call."
        }
    },
    required: ['transactionBlockKindBytes']
} as const;

export const $ExecuteSponsoredTransactionResponse = {
    type: 'object',
    properties: {
        data: {
            type: 'object',
            properties: {
                digest: {
                    type: 'string'
                }
            },
            required: ['digest']
        }
    },
    required: ['data']
} as const;

export const $ExecuteSponsoredTransactionRequest = {
    type: 'object',
    properties: {
        signature: {
            type: 'string',
            description: 'User signature of the transaction.'
        }
    },
    required: ['signature']
} as const;

export const $ZkLogin_Response = {
    type: 'object',
    properties: {
        data: {
            type: 'object',
            properties: {
                salt: {
                    type: 'string'
                },
                address: {
                    type: 'string'
                }
            },
            required: ['salt', 'address']
        }
    },
    required: ['data']
} as const;

export const $App_Response = {
    type: 'object',
    properties: {
        data: {
            type: 'object',
            properties: {
                allowedOrigins: {
                    type: 'array',
                    items: {
                        type: 'string'
                    },
                    example: ['https://example.com']
                },
                authenticationProviders: {
                    type: 'array',
                    items: {
                        type: 'object',
                        properties: {
                            providerType: {
                                type: 'string',
                                enum: ['google', 'facebook', 'twitch', 'apple']
                            },
                            clientId: {
                                type: 'string',
                                nullable: true
                            }
                        },
                        required: ['providerType', 'clientId']
                    },
                    example: [
                        {
                            providerType: 'google',
                            clientId: '...'
                        }
                    ]
                }
            },
            required: ['allowedOrigins', 'authenticationProviders']
        }
    },
    required: ['data']
} as const;
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator

// Tests for representations of all the various system transactions

//# run-graphql
# Query for the genesis transaction
{
    transactionBlocks(first: 1) {
        nodes {
            digest
            sender { address }
            signatures

            gasInput {
                gasSponsor { address }
                gasPayment { nodes { address } }
                gasPrice
                gasBudget
            }

            kind {
                __typename
                ... on GenesisTransaction {
                    objects {
                        edges {
                            cursor
                            node {
                                address

                                asMoveObject {
                                    contents {
                                        type { repr }
                                        json
                                    }
                                }

                                asMovePackage {
                                    modules {
                                        edges {
                                            cursor
                                            node { name }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            effects {
                status
                errors
                lamportVersion
                dependencies {
                    nodes { digest }
                }

                balanceChanges {
                    nodes {
                        owner { address }
                        amount
                        coinType { repr }
                    }
                }

                objectChanges {
                    nodes {
                        address

                        idCreated
                        idDeleted

                        outputState { address digest }
                    }
                }

                gasEffects {
                    gasObject { address }
                    gasSummary {
                        computationCost
                        storageCost
                        storageRebate
                        nonRefundableStorageFee
                    }
                }

                timestamp

                epoch { epochId }
                checkpoint { sequenceNumber }

                transactionBlock { digest }
            }

            expiration { epochId }
        }
    }
}

//# advance-clock --duration-ns 42000000000

//# create-checkpoint

//# run-graphql
# Query for the system transaction that corresponds to a checkpoint (note that
# its timestamp is advanced, because the clock has advanced).
{
    transactionBlocks(last: 1) {
        nodes {
            digest
            sender { address }
            signatures

            gasInput {
                gasSponsor { address }
                gasPayment { nodes { address } }
                gasPrice
                gasBudget
            }

            kind {
                __typename
                ... on ConsensusCommitPrologueTransaction {
                    epoch { epochId }
                    round
                    commitTimestamp
                    consensusCommitDigest
                }
            }

            effects {
                status
                errors
                lamportVersion
                dependencies {
                    nodes { digest }
                }

                balanceChanges {
                    nodes {
                        owner { address }
                        amount
                        coinType { repr }
                    }
                }

                objectChanges {
                    nodes {
                        address

                        idCreated
                        idDeleted

                        outputState { address digest }
                    }
                }

                gasEffects {
                    gasObject { address }
                    gasSummary {
                        computationCost
                        storageCost
                        storageRebate
                        nonRefundableStorageFee
                    }
                }

                timestamp

                epoch { epochId }
                checkpoint { sequenceNumber }

                transactionBlock { digest }
            }

            expiration { epochId }
        }
    }
}

//# advance-clock --duration-ns 43000000000

//# advance-epoch

//# run-graphql
# Look for the change epoch transaction, and again, note the timestamp change.
{
    transactionBlocks(last: 1) {
        nodes {
            digest
            sender { address }
            signatures

            gasInput {
                gasSponsor { address }
                gasPayment { nodes { address } }
                gasPrice
                gasBudget
            }

            kind {
                __typename
                ... on EndOfEpochTransaction {
                    transactions {
                        edges {
                            cursor
                            node {
                                __typename
                                ... on ChangeEpochTransaction {
                                    epoch { epochId }
                                    protocolVersion
                                    storageCharge
                                    computationCharge
                                    storageRebate
                                    nonRefundableStorageFee
                                    startTimestamp
                                }
                            }
                        }
                    }
                }
            }

            effects {
                status
                errors
                lamportVersion
                dependencies {
                    nodes { digest }
                }

                balanceChanges {
                    nodes {
                        owner { address }
                        amount
                        coinType { repr }
                    }
                }

                objectChanges {
                    nodes {
                        address

                        idCreated
                        idDeleted

                        outputState { address digest }
                    }
                }

                gasEffects {
                    gasObject { address }
                    gasSummary {
                        computationCost
                        storageCost
                        storageRebate
                        nonRefundableStorageFee
                    }
                }

                timestamp

                epoch { epochId }
                checkpoint { sequenceNumber }

                transactionBlock { digest }
            }

            expiration { epochId }
        }
    }
}

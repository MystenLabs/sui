// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator

// Tests for representations of all the various system transactions

//# run-graphql
# Query for the genesis transaction
{
    transactionBlockConnection(first: 1) {
        nodes {
            digest
            sender { location }
            signatures { base64Sig }

            gasInput {
                gasSponsor { location }
                gasPayment { nodes { location } }
                gasPrice
                gasBudget
            }

            kind {
                __typename
                ... on GenesisTransaction {
                    objects
                }
            }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { location }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    idCreated
                    idDeleted

                    outputState { location digest }
                }

                gasEffects {
                    gasObject { location }
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
    transactionBlockConnection(last: 1) {
        nodes {
            digest
            sender { location }
            signatures { base64Sig }

            gasInput {
                gasSponsor { location }
                gasPayment { nodes { location } }
                gasPrice
                gasBudget
            }

            kind {
                __typename
                ... on ConsensusCommitPrologueTransaction {
                    epoch { epochId }
                    round
                    timestamp
                }
            }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { location }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    idCreated
                    idDeleted

                    outputState { location digest }
                }

                gasEffects {
                    gasObject { location }
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
    transactionBlockConnection(last: 1) {
        nodes {
            digest
            sender { location }
            signatures { base64Sig }

            gasInput {
                gasSponsor { location }
                gasPayment { nodes { location } }
                gasPrice
                gasBudget
            }

            kind {
                __typename
                ... on ChangeEpochTransaction {
                    epoch { epochId }
                    timestamp
                    storageCharge
                    computationCharge
                    storageRebate
                }
            }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { location }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    idCreated
                    idDeleted

                    outputState { location digest }
                }

                gasEffects {
                    gasObject { location }
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

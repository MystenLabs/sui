// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# publish --upgradeable --sender A
module P0::m {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Foo has key, store {
        id: UID,
        xs: vector<u64>,
    }

    public fun new(xs: vector<u64>, ctx: &mut TxContext): Foo {
        Foo { id: object::new(ctx), xs }
    }
}

//# create-checkpoint

//# run-graphql
# Query for the publish transaction
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
                ... on ProgrammableTransaction {
                    value
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

//# upgrade --package P0 --upgrade-capability 1,1 --sender A
module P0::m {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Foo has key, store {
        id: UID,
        xs: vector<u64>,
    }

    public fun new(xs: vector<u64>, ctx: &mut TxContext): Foo {
        Foo { id: object::new(ctx), xs }
    }

    public fun burn(foo: Foo) {
        let Foo { id, xs: _ } = foo;
        object::delete(id);
    }
}

//# create-checkpoint

//# run-graphql

# Query for the upgrade transaction
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
                ... on ProgrammableTransaction {
                    value
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

//# programmable --sender A --inputs 42u64 43u64 1000 @A
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: MakeMoveVec<u64>([]);
//> 2: SplitCoins(Gas, [Input(2), Input(2)]);
//> 3: P0::m::new(Result(0));
//> 4: TransferObjects([Result(3)], Input(3));
//> 5: P0::m::new(Result(1));
//> 6: P0::m::burn(Result(5));
//> 7: MergeCoins(NestedResult(2,0), [NestedResult(2,1)]);
//> TransferObjects([NestedResult(2,0)], Input(3))

//# create-checkpoint

//# run-graphql

# Query for the programmable transaction
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
                ... on ProgrammableTransaction {
                    value
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

                    outputState {
                        location
                        digest
                        asMoveObject {
                            contents {
                                type { repr }
                                json
                            }
                        }
                    }
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

//# programmable --sender A --inputs 1000
//> SplitCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-graphql

# Query for the programmable transaction, which failed.
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
                ... on ProgrammableTransaction {
                    value
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

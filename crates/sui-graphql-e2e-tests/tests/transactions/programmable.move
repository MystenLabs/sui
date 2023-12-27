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

fragment ObjectContent on Object {
    address
    version
    digest
    asMoveObject {
        contents {
            type { repr }
            json
        }
    }
}

fragment TxInput on TransactionInput {
    __typename

    ... on OwnedOrImmutable {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on SharedInput {
        address
        initialSharedVersion
        mutable
    }

    ... on Receiving {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on Pure {
        bytes
    }
}

fragment TxArg on TransactionArgument {
    __typename
    ... on Input { ix }
    ... on Result { cmd ix }
}

fragment Tx on ProgrammableTransaction {
    __typename

    ... on MoveCallTransaction {
        package
        module
        functionName
        typeArguments { repr }
        arguments { ...TxArg }

        function {
            isEntry
            typeParameters { constraints }
            parameters { repr }
            return { repr }
        }
    }

    ... on TransferObjectsTransaction {
        inputs { ...TxArg }
        address { ...TxArg }
    }

    ... on SplitCoinsTransaction {
        coin { ...TxArg }
        amounts { ...TxArg }
    }

    ... on MergeCoinsTransaction {
        coin { ...TxArg }
        coins { ...TxArg }
    }

    ... on PublishTransaction {
        modules
        dependencies
    }

    ... on UpgradeTransaction {
        modules
        dependencies
        currentPackage
        upgradeTicket { ...TxArg }
    }

    ... on MakeMoveVecTransaction {
        type { repr }
        elements { ...TxArg }
    }
}

fragment ComprehensivePTB on ProgrammableTransactionBlock {
    inputs {
        edges {
            cursor
            node { __typename ...TxInput }
        }
    }
    transactions {
        edges {
            cursor
            node { __typename ...Tx }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
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

            kind { __typename ...ComprehensivePTB }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { address }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    address

                    idCreated
                    idDeleted

                    outputState { address digest }
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

fragment ObjectContent on Object {
    address
    version
    digest
    asMoveObject {
        contents {
            type { repr }
            json
        }
    }
}

fragment TxInput on TransactionInput {
    __typename

    ... on OwnedOrImmutable {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on SharedInput {
        address
        initialSharedVersion
        mutable
    }

    ... on Receiving {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on Pure {
        bytes
    }
}

fragment TxArg on TransactionArgument {
    __typename
    ... on Input { ix }
    ... on Result { cmd ix }
}

fragment Tx on ProgrammableTransaction {
    __typename

    ... on MoveCallTransaction {
        package
        module
        functionName
        typeArguments { repr }
        arguments { ...TxArg }

        function {
            isEntry
            typeParameters { constraints }
            parameters { repr }
            return { repr }
        }
    }

    ... on TransferObjectsTransaction {
        inputs { ...TxArg }
        address { ...TxArg }
    }

    ... on SplitCoinsTransaction {
        coin { ...TxArg }
        amounts { ...TxArg }
    }

    ... on MergeCoinsTransaction {
        coin { ...TxArg }
        coins { ...TxArg }
    }

    ... on PublishTransaction {
        modules
        dependencies
    }

    ... on UpgradeTransaction {
        modules
        dependencies
        currentPackage
        upgradeTicket { ...TxArg }
    }

    ... on MakeMoveVecTransaction {
        type { repr }
        elements { ...TxArg }
    }
}

fragment ComprehensivePTB on ProgrammableTransactionBlock {
    inputs {
        edges {
            cursor
            node { __typename ...TxInput }
        }
    }
    transactions {
        edges {
            cursor
            node { __typename ...Tx }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
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

            kind { __typename ...ComprehensivePTB }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { address }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    address

                    idCreated
                    idDeleted

                    outputState { address digest }
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

fragment ObjectContent on Object {
    address
    version
    digest
    asMoveObject {
        contents {
            type { repr }
            json
        }
    }
}

fragment TxInput on TransactionInput {
    __typename

    ... on OwnedOrImmutable {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on SharedInput {
        address
        initialSharedVersion
        mutable
    }

    ... on Receiving {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on Pure {
        bytes
    }
}

fragment TxArg on TransactionArgument {
    __typename
    ... on Input { ix }
    ... on Result { cmd ix }
}

fragment Tx on ProgrammableTransaction {
    __typename

    ... on MoveCallTransaction {
        package
        module
        functionName
        typeArguments { repr }
        arguments { ...TxArg }

        function {
            isEntry
            typeParameters { constraints }
            parameters { repr }
            return { repr }
        }
    }

    ... on TransferObjectsTransaction {
        inputs { ...TxArg }
        address { ...TxArg }
    }

    ... on SplitCoinsTransaction {
        coin { ...TxArg }
        amounts { ...TxArg }
    }

    ... on MergeCoinsTransaction {
        coin { ...TxArg }
        coins { ...TxArg }
    }

    ... on PublishTransaction {
        modules
        dependencies
    }

    ... on UpgradeTransaction {
        modules
        dependencies
        currentPackage
        upgradeTicket { ...TxArg }
    }

    ... on MakeMoveVecTransaction {
        type { repr }
        elements { ...TxArg }
    }
}

fragment ComprehensivePTB on ProgrammableTransactionBlock {
    inputs {
        edges {
            cursor
            node { __typename ...TxInput }
        }
    }
    transactions {
        edges {
            cursor
            node { __typename ...Tx }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
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

            kind { __typename ...ComprehensivePTB }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { address }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    address

                    idCreated
                    idDeleted

                    outputState {
                        address
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

//# programmable --sender A --inputs 1000
//> SplitCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-graphql
# Query for the programmable transaction, which failed.

fragment ObjectContent on Object {
    address
    version
    digest
    asMoveObject {
        contents {
            type { repr }
            json
        }
    }
}

fragment TxInput on TransactionInput {
    __typename

    ... on OwnedOrImmutable {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on SharedInput {
        address
        initialSharedVersion
        mutable
    }

    ... on Receiving {
        address
        version
        digest
        object { ...ObjectContent }
    }

    ... on Pure {
        bytes
    }
}

fragment TxArg on TransactionArgument {
    __typename
    ... on Input { ix }
    ... on Result { cmd ix }
}

fragment Tx on ProgrammableTransaction {
    __typename

    ... on MoveCallTransaction {
        package
        module
        functionName
        typeArguments { repr }
        arguments { ...TxArg }

        function {
            isEntry
            typeParameters { constraints }
            parameters { repr }
            return { repr }
        }
    }

    ... on TransferObjectsTransaction {
        inputs { ...TxArg }
        address { ...TxArg }
    }

    ... on SplitCoinsTransaction {
        coin { ...TxArg }
        amounts { ...TxArg }
    }

    ... on MergeCoinsTransaction {
        coin { ...TxArg }
        coins { ...TxArg }
    }

    ... on PublishTransaction {
        modules
        dependencies
    }

    ... on UpgradeTransaction {
        modules
        dependencies
        currentPackage
        upgradeTicket { ...TxArg }
    }

    ... on MakeMoveVecTransaction {
        type { repr }
        elements { ...TxArg }
    }
}

fragment ComprehensivePTB on ProgrammableTransactionBlock {
    inputs {
        edges {
            cursor
            node { __typename ...TxInput }
        }
    }
    transactions {
        edges {
            cursor
            node { __typename ...Tx }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
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

            kind { __typename ...ComprehensivePTB }

            effects {
                status
                errors
                lamportVersion
                dependencies { digest }

                balanceChanges {
                    owner { address }
                    amount
                    coinType { repr }
                }

                objectChanges {
                    address

                    idCreated
                    idDeleted

                    outputState { address digest }
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

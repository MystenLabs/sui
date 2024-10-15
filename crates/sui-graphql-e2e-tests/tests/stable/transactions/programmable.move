// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# publish --upgradeable --sender A
module P0::m {
    public struct Foo has key, store {
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

            kind { __typename ...ComprehensivePTB }

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

//# upgrade --package P0 --upgrade-capability 1,1 --sender A
module P0::m {
    public struct Foo has key, store {
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

            kind { __typename ...ComprehensivePTB }

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

//# programmable --sender A --inputs 41u64 @A
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: P0::m::new(Result(0));
//> sui::transfer::public_transfer<P0::m::Foo>(Result(1), Input(1))

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

            kind { __typename ...ComprehensivePTB }

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

            kind { __typename ...ComprehensivePTB }

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

//# run-graphql
{ # All transactions
    transactionBlocks(last: 10) {
        edges {
            cursor
            node {
                kind { __typename }
            }
        }
    }
}

//# run-graphql
{ # System transactions
    transactionBlocks(last: 10, filter: { kind: SYSTEM_TX }) {
        edges {
            cursor
            node {
                kind { __typename }
            }
        }
    }
}

//# run-graphql
{ # Non-system transactions
    transactionBlocks(last: 10, filter: { kind: PROGRAMMABLE_TX }) {
        edges {
            cursor
            node {
                kind { __typename }
            }
        }
    }
}

//# run-graphql
{ # Conflicting filter and context
    address(address: "@{A}") {
        transactionBlocks(last: 10, filter: { sentAddress: "0x0" }) {
            nodes { kind { __typename } }
        }
    }
}

//# run-graphql
{ # Filtering by function package
    transactionBlocks(last: 10, filter: { function: "0x2" }) {
        edges { cursor }
    }
}

//# run-graphql
{ # Filtering by function module
    transactionBlocks(last: 10, filter: { function: "@{P0}::m" }) {
        edges { cursor }
    }
}

//# run-graphql
{ # Filtering by function
    transactionBlocks(
        last: 10,
        filter: {
            function: "0x2::transfer::public_transfer"
        }
    ) {
        edges { cursor }
    }
}

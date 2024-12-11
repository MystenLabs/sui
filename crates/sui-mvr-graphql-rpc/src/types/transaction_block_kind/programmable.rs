// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    consistency::ConsistentIndexCursor,
    types::{
        base64::Base64,
        cursor::{JsonCursor, Page},
        move_function::MoveFunction,
        move_type::MoveType,
        object_read::ObjectRead,
        sui_address::SuiAddress,
        uint53::UInt53,
    },
};
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use sui_json_rpc_types::SuiArgument;
use sui_types::transaction::{
    Argument as NativeArgument, CallArg as NativeCallArg, Command as NativeProgrammableTransaction,
    ObjectArg as NativeObjectArg, ProgrammableMoveCall as NativeMoveCallTransaction,
    ProgrammableTransaction as NativeProgrammableTransactionBlock,
};

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ProgrammableTransactionBlock {
    pub native: NativeProgrammableTransactionBlock,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

pub(crate) type CInput = JsonCursor<ConsistentIndexCursor>;
pub(crate) type CTxn = JsonCursor<ConsistentIndexCursor>;

#[derive(Union, Clone, Eq, PartialEq)]
enum TransactionInput {
    OwnedOrImmutable(OwnedOrImmutable),
    SharedInput(SharedInput),
    Receiving(Receiving),
    Pure(Pure),
}

/// A Move object, either immutable, or owned mutable.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct OwnedOrImmutable {
    #[graphql(flatten)]
    read: ObjectRead,
}

/// A Move object that's shared.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct SharedInput {
    address: SuiAddress,
    /// The version that this this object was shared at.
    initial_shared_version: UInt53,
    /// Controls whether the transaction block can reference the shared object as a mutable
    /// reference or by value. This has implications for scheduling: Transactions that just read
    /// shared objects at a certain version (mutable = false) can be executed concurrently, while
    /// transactions that write shared objects (mutable = true) must be executed serially with
    /// respect to each other.
    mutable: bool,
}

/// A Move object that can be received in this transaction.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct Receiving {
    #[graphql(flatten)]
    read: ObjectRead,
}

/// BCS encoded primitive value (not an object or Move struct).
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct Pure {
    /// BCS serialized and Base64 encoded primitive value.
    bytes: Base64,
}

/// A single transaction, or command, in the programmable transaction block.
#[derive(Union, Clone, Eq, PartialEq)]
enum ProgrammableTransaction {
    MoveCall(MoveCallTransaction),
    TransferObjects(TransferObjectsTransaction),
    SplitCoins(SplitCoinsTransaction),
    MergeCoins(MergeCoinsTransaction),
    Publish(PublishTransaction),
    Upgrade(UpgradeTransaction),
    MakeMoveVec(MakeMoveVecTransaction),
}

#[derive(Clone, Eq, PartialEq)]
struct MoveCallTransaction {
    native: NativeMoveCallTransaction,
    checkpoint_viewed_at: u64,
}

/// Transfers `inputs` to `address`. All inputs must have the `store` ability (allows public
/// transfer) and must not be previously immutable or shared.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct TransferObjectsTransaction {
    /// The objects to transfer.
    inputs: Vec<TransactionArgument>,

    /// The address to transfer to.
    address: TransactionArgument,
}

/// Splits off coins with denominations in `amounts` from `coin`, returning multiple results (as
/// many as there are amounts.)
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct SplitCoinsTransaction {
    /// The coin to split.
    coin: TransactionArgument,

    /// The denominations to split off from the coin.
    amounts: Vec<TransactionArgument>,
}

/// Merges `coins` into the first `coin` (produces no results).
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct MergeCoinsTransaction {
    /// The coin to merge into.
    coin: TransactionArgument,

    /// The coins to be merged.
    coins: Vec<TransactionArgument>,
}

/// Publishes a Move Package.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct PublishTransaction {
    /// Bytecode for the modules to be published, BCS serialized and Base64 encoded.
    modules: Vec<Base64>,

    /// IDs of the transitive dependencies of the package to be published.
    dependencies: Vec<SuiAddress>,
}

/// Upgrades a Move Package.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct UpgradeTransaction {
    /// Bytecode for the modules to be published, BCS serialized and Base64 encoded.
    modules: Vec<Base64>,

    /// IDs of the transitive dependencies of the package to be published.
    dependencies: Vec<SuiAddress>,

    /// ID of the package being upgraded.
    current_package: SuiAddress,

    /// The `UpgradeTicket` authorizing the upgrade.
    upgrade_ticket: TransactionArgument,
}

/// Create a vector (possibly empty).
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct MakeMoveVecTransaction {
    /// If the elements are not objects, or the vector is empty, a type must be supplied.
    #[graphql(name = "type")]
    type_: Option<MoveType>,

    /// The values to pack into the vector, all of the same type.
    elements: Vec<TransactionArgument>,
}

/// An argument to a programmable transaction command.
#[derive(Union, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TransactionArgument {
    GasCoin(GasCoin),
    Input(Input),
    Result(TxResult),
}

/// Access to the gas inputs, after they have been smashed into one coin. The gas coin can only be
/// used by reference, except for with `TransferObjectsTransaction` that can accept it by value.
#[derive(SimpleObject, Clone, Debug, Eq, PartialEq)]
pub(crate) struct GasCoin {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// One of the input objects or primitive values to the programmable transaction block.
#[derive(SimpleObject, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Input {
    /// Index of the programmable transaction block input (0-indexed).
    ix: u16,
}

/// The result of another transaction command.
#[derive(SimpleObject, Clone, Debug, Eq, PartialEq)]
#[graphql(name = "Result")]
pub(crate) struct TxResult {
    /// The index of the previous command (0-indexed) that returned this result.
    cmd: u16,

    /// If the previous command returns multiple values, this is the index of the individual result
    /// among the multiple results from that command (also 0-indexed).
    ix: Option<u16>,
}

/// A user transaction that allows the interleaving of native commands (like transfer, split coins,
/// merge coins, etc) and move calls, executed atomically.
#[Object]
impl ProgrammableTransactionBlock {
    /// Input objects or primitive values.
    async fn inputs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CInput>,
        last: Option<u64>,
        before: Option<CInput>,
    ) -> Result<Connection<String, TransactionInput>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let mut connection = Connection::new(false, false);
        let Some((prev, next, _, cs)) =
            page.paginate_consistent_indices(self.native.inputs.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let input = TransactionInput::from(self.native.inputs[c.ix].clone(), c.c);
            connection.edges.push(Edge::new(c.encode_cursor(), input));
        }

        Ok(connection)
    }

    /// The transaction commands, executed sequentially.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTxn>,
        last: Option<u64>,
        before: Option<CTxn>,
    ) -> Result<Connection<String, ProgrammableTransaction>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let mut connection = Connection::new(false, false);
        let Some((prev, next, _, cs)) = page
            .paginate_consistent_indices(self.native.commands.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let txn = ProgrammableTransaction::from(self.native.commands[c.ix].clone(), c.c);
            connection.edges.push(Edge::new(c.encode_cursor(), txn));
        }

        Ok(connection)
    }
}

/// A call to either an entry or a public Move function.
#[Object]
impl MoveCallTransaction {
    /// The storage ID of the package the function being called is defined in.
    async fn package(&self) -> SuiAddress {
        self.native.package.into()
    }

    /// The name of the module the function being called is defined in.
    async fn module(&self) -> &str {
        self.native.module.as_str()
    }

    /// The name of the function being called.
    async fn function_name(&self) -> &str {
        self.native.function.as_str()
    }

    /// The function being called, resolved.
    async fn function(&self, ctx: &Context<'_>) -> Result<Option<MoveFunction>> {
        MoveFunction::query(
            ctx,
            self.native.package.into(),
            self.native.module.as_str(),
            self.native.function.as_str(),
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    /// The actual type parameters passed in for this move call.
    async fn type_arguments(&self) -> Vec<MoveType> {
        self.native
            .type_arguments
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    /// The actual function parameters passed in for this move call.
    async fn arguments(&self) -> Vec<TransactionArgument> {
        self.native
            .arguments
            .iter()
            .map(|arg| TransactionArgument::from(*arg))
            .collect()
    }
}

impl TransactionInput {
    fn from(argument: NativeCallArg, checkpoint_viewed_at: u64) -> Self {
        use NativeCallArg as N;
        use NativeObjectArg as O;
        use TransactionInput as I;

        match argument {
            N::Pure(bytes) => I::Pure(Pure {
                bytes: Base64::from(bytes),
            }),

            N::Object(O::ImmOrOwnedObject(oref)) => I::OwnedOrImmutable(OwnedOrImmutable {
                read: ObjectRead {
                    native: oref,
                    checkpoint_viewed_at,
                },
            }),

            N::Object(O::SharedObject {
                id,
                initial_shared_version,
                mutable,
            }) => I::SharedInput(SharedInput {
                address: id.into(),
                initial_shared_version: initial_shared_version.value().into(),
                mutable,
            }),

            N::Object(O::Receiving(oref)) => I::Receiving(Receiving {
                read: ObjectRead {
                    native: oref,
                    checkpoint_viewed_at,
                },
            }),
        }
    }
}

impl ProgrammableTransaction {
    fn from(pt: NativeProgrammableTransaction, checkpoint_viewed_at: u64) -> Self {
        use NativeProgrammableTransaction as N;
        use ProgrammableTransaction as P;
        match pt {
            N::MoveCall(call) => P::MoveCall(MoveCallTransaction {
                native: *call,
                checkpoint_viewed_at,
            }),

            N::TransferObjects(inputs, address) => P::TransferObjects(TransferObjectsTransaction {
                inputs: inputs.into_iter().map(TransactionArgument::from).collect(),
                address: address.into(),
            }),

            N::SplitCoins(coin, amounts) => P::SplitCoins(SplitCoinsTransaction {
                coin: coin.into(),
                amounts: amounts.into_iter().map(TransactionArgument::from).collect(),
            }),

            N::MergeCoins(coin, coins) => P::MergeCoins(MergeCoinsTransaction {
                coin: coin.into(),
                coins: coins.into_iter().map(TransactionArgument::from).collect(),
            }),

            N::Publish(modules, dependencies) => P::Publish(PublishTransaction {
                modules: modules.into_iter().map(Base64::from).collect(),
                dependencies: dependencies.into_iter().map(SuiAddress::from).collect(),
            }),

            N::MakeMoveVec(type_, elements) => P::MakeMoveVec(MakeMoveVecTransaction {
                type_: type_.map(Into::into),
                elements: elements
                    .into_iter()
                    .map(TransactionArgument::from)
                    .collect(),
            }),

            N::Upgrade(modules, dependencies, current_package, upgrade_ticket) => {
                P::Upgrade(UpgradeTransaction {
                    modules: modules.into_iter().map(Base64::from).collect(),
                    dependencies: dependencies.into_iter().map(SuiAddress::from).collect(),
                    current_package: current_package.into(),
                    upgrade_ticket: upgrade_ticket.into(),
                })
            }
        }
    }
}

impl From<NativeArgument> for TransactionArgument {
    fn from(argument: NativeArgument) -> Self {
        use NativeArgument as N;
        use TransactionArgument as A;
        match argument {
            N::GasCoin => A::GasCoin(GasCoin { dummy: None }),
            N::Input(ix) => A::Input(Input { ix }),
            N::Result(cmd) => A::Result(TxResult { cmd, ix: None }),
            N::NestedResult(cmd, ix) => A::Result(TxResult { cmd, ix: Some(ix) }),
        }
    }
}

impl From<SuiArgument> for TransactionArgument {
    fn from(argument: SuiArgument) -> Self {
        use SuiArgument as S;
        use TransactionArgument as A;
        match argument {
            S::GasCoin => A::GasCoin(GasCoin { dummy: None }),
            S::Input(ix) => A::Input(Input { ix }),
            S::Result(cmd) => A::Result(TxResult { cmd, ix: None }),
            S::NestedResult(cmd, ix) => A::Result(TxResult { cmd, ix: Some(ix) }),
        }
    }
}

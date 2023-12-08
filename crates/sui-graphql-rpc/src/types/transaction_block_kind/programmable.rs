// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, Edge},
    *,
};
use sui_types::transaction::{
    Argument as NativeArgument, CallArg as NativeCallArg, Command as NativeProgrammableTransaction,
    ObjectArg as NativeObjectArg, ProgrammableMoveCall as NativeMoveCallTransaction,
    ProgrammableTransaction as NativeProgrammableTransactionBlock,
};

use crate::{
    context_data::db_data_provider::{validate_cursor_pagination, PgManager},
    error::Error,
    types::{
        base64::Base64, move_function::MoveFunction, move_type::MoveType, object_read::ObjectRead,
        sui_address::SuiAddress,
    },
};

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ProgrammableTransactionBlock(pub NativeProgrammableTransactionBlock);

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
    initial_shared_version: u64,
    /// Controls whether the transaction block can reference the shared object as a mutable
    /// reference or by value.
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
struct MoveCallTransaction(NativeMoveCallTransaction);

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
#[derive(Union, Clone, Eq, PartialEq)]
enum TransactionArgument {
    GasCoin(GasCoin),
    Input(Input),
    Result(TxResult),
}

/// Access to the gas inputs, after they have been smashed into one coin. The gas coin can only be
/// used by reference, except for with `TransferObjectsTransaction` that can accept it by value.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct GasCoin {
    /// A workaround to define an empty variant of a GraphQL union.
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// One of the input objects or primitive values to the programmable transaction block.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
struct Input {
    /// Index of the programmable transaction block input (0-indexed).
    ix: u16,
}

/// The result of another transaction command.
#[derive(SimpleObject, Clone, Eq, PartialEq)]
#[graphql(name = "Result")]
struct TxResult {
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
    async fn input_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, TransactionInput>> {
        // TODO: make cursor opaque (currently just an offset).
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let total = self.0.inputs.len();

        let mut lo = if let Some(after) = after {
            1 + after
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'after' cursor.".to_string()))
                .extend()?
        } else {
            0
        };

        let mut hi = if let Some(before) = before {
            before
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'before' cursor.".to_string()))
                .extend()?
        } else {
            total
        };

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        }

        // If there's a `first` limit, bound the upperbound to be at most `first` away from the
        // lowerbound.
        if let Some(first) = first {
            let first = first as usize;
            if hi - lo > first {
                hi = lo + first;
            }
        }

        // If there's a `last` limit, bound the lowerbound to be at most `last` away from the
        // upperbound.  NB. This applies after we bounded the upperbound, using `first`.
        if let Some(last) = last {
            let last = last as usize;
            if hi - lo > last {
                lo = hi - last;
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        for (idx, input) in self.0.inputs.iter().enumerate().skip(lo).take(hi - lo) {
            let input = TransactionInput::from(input.clone());
            connection.edges.push(Edge::new(idx.to_string(), input));
        }

        Ok(connection)
    }

    /// The transaction commands, executed sequentially.
    async fn transaction_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, ProgrammableTransaction>> {
        // TODO: make cursor opaque (currently just an offset).
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let total = self.0.commands.len();

        let mut lo = if let Some(after) = after {
            1 + after
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'after' cursor.".to_string()))
                .extend()?
        } else {
            0
        };

        let mut hi = if let Some(before) = before {
            before
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'before' cursor.".to_string()))
                .extend()?
        } else {
            total
        };

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        }

        // If there's a `first` limit, bound the upperbound to be at most `first` away from the
        // lowerbound.
        if let Some(first) = first {
            let first = first as usize;
            if hi - lo > first {
                hi = lo + first;
            }
        }

        // If there's a `last` limit, bound the lowerbound to be at most `last` away from the
        // upperbound.  NB. This applies after we bounded the upperbound, using `first`.
        if let Some(last) = last {
            let last = last as usize;
            if hi - lo > last {
                lo = hi - last;
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        for (idx, cmd) in self.0.commands.iter().enumerate().skip(lo).take(hi - lo) {
            let input = ProgrammableTransaction::from(cmd.clone());
            connection.edges.push(Edge::new(idx.to_string(), input));
        }

        Ok(connection)
    }
}

/// A call to either an entry or a public Move function.
#[Object]
impl MoveCallTransaction {
    /// The storage ID of the package the function being called is defined in.
    async fn package(&self) -> SuiAddress {
        self.0.package.into()
    }

    /// The name of the module the function being called is defined in.
    async fn module(&self) -> &str {
        self.0.module.as_str()
    }

    /// The name of the function being called.
    async fn function_name(&self) -> &str {
        self.0.function.as_str()
    }

    /// The function being called, resolved.
    async fn function(&self, ctx: &Context<'_>) -> Result<Option<MoveFunction>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_function(
                self.0.package.into(),
                self.0.module.as_str(),
                self.0.function.as_str(),
            )
            .await
            .extend()
    }

    /// The actual type parameters passed in for this move call.
    async fn type_arguments(&self) -> Vec<MoveType> {
        self.0
            .type_arguments
            .iter()
            .map(|tag| MoveType::new(tag.clone()))
            .collect()
    }

    /// The actual function parameters passed in for this move call.
    async fn arguments(&self) -> Vec<TransactionArgument> {
        self.0
            .arguments
            .iter()
            .map(|arg| TransactionArgument::from(*arg))
            .collect()
    }
}

impl From<NativeCallArg> for TransactionInput {
    fn from(argument: NativeCallArg) -> Self {
        use NativeCallArg as N;
        use NativeObjectArg as O;
        use TransactionInput as I;

        match argument {
            N::Pure(bytes) => I::Pure(Pure {
                bytes: Base64::from(bytes),
            }),

            N::Object(O::ImmOrOwnedObject(oref)) => I::OwnedOrImmutable(OwnedOrImmutable {
                read: ObjectRead(oref),
            }),

            N::Object(O::SharedObject {
                id,
                initial_shared_version,
                mutable,
            }) => I::SharedInput(SharedInput {
                address: id.into(),
                initial_shared_version: initial_shared_version.value(),
                mutable,
            }),

            N::Object(O::Receiving(oref)) => I::Receiving(Receiving {
                read: ObjectRead(oref),
            }),
        }
    }
}

impl From<NativeProgrammableTransaction> for ProgrammableTransaction {
    fn from(pt: NativeProgrammableTransaction) -> Self {
        use NativeProgrammableTransaction as N;
        use ProgrammableTransaction as P;
        match pt {
            N::MoveCall(call) => P::MoveCall(MoveCallTransaction(*call)),

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
                type_: type_.map(MoveType::new),
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The model of how a transaction pays for gas.
//!
//! `GasData` describes gas payment on the wire in three overloaded forms: a single sentinel
//! `ObjectRef` means "unmetered", an empty payment list means "draw the whole budget from the gas
//! owner's address balance", and each entry of a non-empty list is either a real coin `ObjectRef`
//! or an address-balance reservation encoded as a fake `ObjectRef` (see
//! `sui_types::coin_reservation`). This module is the only place in the execution layer that
//! decodes those forms. Everything downstream works with `PaymentKind` and `PaymentMethod`.

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub mod checked {
    use either::Either;
    use indexmap::IndexMap;
    use nonempty::NonEmpty;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::{
        base_types::{ObjectID, ObjectRef, SuiAddress},
        coin_reservation::ParsedDigest,
        error::ExecutionError,
        gas_model::gas_predicates::bump_only_enabled,
        transaction::{GasData, TransactionKind, is_gasless_transaction},
    };

    /// A single source of SUI used to pay for gas: either a coin object or a withdrawal
    /// reservation from an address balance.
    #[derive(Debug, Clone)]
    pub enum PaymentMethod {
        Coin(ObjectRef),
        AddressBalance(SuiAddress, /* withdrawal reservation */ u64),
    }

    /// Identifies where a gas payment lives, independent of its value (`ObjectRef` or reservation).
    /// Used often as a key, e.g. during smashing and during gas final charging.
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum PaymentLocation {
        Coin(ObjectID),
        AddressBalance(SuiAddress),
    }

    /// A resolved gas payment: the location that will receive the final charge or refund,
    /// paired with the total SUI available after smashing. Produced by
    /// `GasCharger::gas_payment` and consumed by PTB execution to set up the runtime gas coin.
    #[derive(Debug, Clone, Copy)]
    pub struct GasPayment {
        /// The location of the gas payment (coin or address balance), which also serves as the
        /// target for smashed gas payments.
        pub location: PaymentLocation,
        /// The total amount available for gas payment after smashing
        pub amount: u64,
    }

    /// How a transaction's gas is paid, decoded from its `GasData` and kind. Built once per
    /// transaction by `PaymentKind::from_transaction` and shared by the temporary store (for
    /// input reservations) and the gas charger (via `check`, for smashing and charging), so both
    /// see the same classification.
    #[derive(Debug, Clone)]
    pub enum PaymentKind {
        /// No payment and no metering: system transactions and dev-inspect.
        Unmetered,
        /// Metered but free: gas is measured but nothing is charged.
        Gasless,
        /// One or more user-provided sources, smashed into the first one at the start of
        /// execution.
        Metered(NonEmpty<PaymentMethod>),
    }

    /// A `PaymentKind` whose metered sources passed `PaymentKind::check`: unique by location,
    /// with reservations from the same address balance merged. Consumed by `GasCharger::new`.
    #[derive(Debug)]
    pub struct CheckedPaymentKind(pub(crate) CheckedPaymentKindInner);

    #[derive(Debug)]
    pub(crate) enum CheckedPaymentKindInner {
        Unmetered,
        Gasless,
        Metered {
            /// The first source. It receives the value of all the others and is the initial gas
            /// charge location.
            smash_target: PaymentMethod,
            /// The remaining sources, keyed by location. Does not contain the target's location.
            smashed_payments: IndexMap<PaymentLocation, PaymentMethod>,
        },
    }

    impl PaymentKind {
        pub fn from_transaction(
            gas_data: &GasData,
            transaction_kind: &TransactionKind,
            protocol_config: &ProtocolConfig,
        ) -> Self {
            // Gasless classification is unconditional at gas model v15+; below it, it is behind the
            // feature flag.
            let gasless_enabled = bump_only_enabled(protocol_config.gas_model_version())
                || protocol_config.enable_gasless();
            if gas_data.is_unmetered() || transaction_kind.is_system_tx() {
                Self::Unmetered
            } else if gasless_enabled && is_gasless_transaction(gas_data, transaction_kind) {
                Self::Gasless
            } else {
                let decode = |entry| PaymentMethod::from_gas_payment_entry(gas_data.owner, entry);
                Self::Metered(match gas_data.payment.split_first() {
                    // No explicit payment: the whole budget comes from the owner's balance.
                    None => NonEmpty::new(PaymentMethod::AddressBalance(
                        gas_data.owner,
                        gas_data.budget,
                    )),
                    Some((smash_target, rest)) => NonEmpty {
                        head: decode(smash_target),
                        tail: rest.iter().map(decode).collect(),
                    },
                })
            }
        }

        pub fn is_gasless(&self) -> bool {
            matches!(self, Self::Gasless)
        }

        /// Groups the metered sources by location. Errors on an invalid payment set: a duplicate
        /// gas coin, or an overflowing address-balance reservation sum. Input checks reject both
        /// at signing, so an error here is an invariant violation.
        pub fn check(self) -> Result<CheckedPaymentKind, ExecutionError> {
            let payment_methods = match self {
                Self::Unmetered => {
                    return Ok(CheckedPaymentKind(CheckedPaymentKindInner::Unmetered));
                }
                Self::Gasless => return Ok(CheckedPaymentKind(CheckedPaymentKindInner::Gasless)),
                Self::Metered(payment_methods) => payment_methods,
            };
            let mut unique_methods: IndexMap<PaymentLocation, PaymentMethod> = IndexMap::new();
            for payment_method in payment_methods {
                match (
                    unique_methods.entry(payment_method.location()),
                    payment_method,
                ) {
                    (indexmap::map::Entry::Vacant(entry), payment_method) => {
                        entry.insert(payment_method);
                    }
                    (
                        indexmap::map::Entry::Occupied(mut occupied),
                        PaymentMethod::AddressBalance(other, additional),
                    ) => {
                        let PaymentMethod::AddressBalance(addr, amount) = occupied.get_mut() else {
                            unreachable!("Payment method does not match location")
                        };
                        assert_eq!(*addr, other, "Payment method does not match location");
                        *amount = amount.checked_add(additional).ok_or_else(|| {
                            ExecutionError::invariant_violation(
                                "address-balance gas reservation overflow",
                            )
                        })?;
                    }
                    (indexmap::map::Entry::Occupied(_), PaymentMethod::Coin(_)) => {
                        return Err(ExecutionError::invariant_violation("duplicate gas coin"));
                    }
                }
            }
            // The input is non-empty and merging never removes the first entry, so the smash
            // target is always present.
            let (_, smash_target) = unique_methods
                .shift_remove_index(0)
                .expect("non-empty payment methods have a smash target");
            Ok(CheckedPaymentKind(CheckedPaymentKindInner::Metered {
                smash_target,
                smashed_payments: unique_methods,
            }))
        }

        /// Every address-balance source, as `(owner, maximum withdrawal)`. Entries for the same
        /// owner are not merged. Empty for unmetered and gasless payments.
        pub fn address_balance_reservations(&self) -> impl Iterator<Item = (SuiAddress, u64)> + '_ {
            match self {
                Self::Unmetered | Self::Gasless => Either::Left(std::iter::empty()),
                Self::Metered(methods) => Either::Right(methods.iter()),
            }
            .filter_map(|method| method.address_balance())
        }
    }

    impl PaymentMethod {
        /// Decodes one entry of `GasData::payment`. An address-balance reservation is encoded as a
        /// fake `ObjectRef` whose digest carries the reservation amount; anything else is a coin.
        pub fn from_gas_payment_entry(gas_owner: SuiAddress, entry: &ObjectRef) -> Self {
            match ParsedDigest::try_from(entry.2) {
                Ok(parsed) => Self::AddressBalance(gas_owner, parsed.reservation_amount()),
                Err(_) => Self::Coin(*entry),
            }
        }

        pub fn location(&self) -> PaymentLocation {
            match self {
                Self::Coin(obj_ref) => PaymentLocation::Coin(obj_ref.0),
                Self::AddressBalance(addr, _) => PaymentLocation::AddressBalance(*addr),
            }
        }

        pub fn is_coin(&self) -> bool {
            matches!(self, Self::Coin(_))
        }

        pub fn as_coin(&self) -> Option<&ObjectRef> {
            match self {
                Self::Coin(obj_ref) => Some(obj_ref),
                Self::AddressBalance(..) => None,
            }
        }

        pub fn address_balance(&self) -> Option<(SuiAddress, u64)> {
            match self {
                Self::Coin(_) => None,
                Self::AddressBalance(addr, reservation) => Some((*addr, *reservation)),
            }
        }
    }

    impl PaymentLocation {
        pub fn coin(&self) -> Option<ObjectID> {
            match self {
                Self::Coin(id) => Some(*id),
                Self::AddressBalance(_) => None,
            }
        }

        pub fn address_balance(&self) -> Option<SuiAddress> {
            match self {
                Self::Coin(_) => None,
                Self::AddressBalance(addr) => Some(*addr),
            }
        }
    }
}

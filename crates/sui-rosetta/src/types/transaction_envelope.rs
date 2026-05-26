// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Wire format for `unsigned_transaction` / `signed_transaction`.
//!
//! A JSON wrapper around the construction-endpoint opaque blobs, carrying:
//!
//! 1. The proto representation of the transaction
//!    (`hex(prost::encode(proto::Transaction))`). Structured fields (`kind`,
//!    `sender`, `gas_payment`, `expiration`) are populated; the opaque `bcs`
//!    field is cleared at encode time and rejected at decode time.
//! 2. The minimal `AuxData` the PTB cannot encode (PayCoin
//!    currency, FSS validator, AtMost redeem cap). `/parse` reconstructs
//!    `Operations` *from the transaction* and applies these labels. The aux
//!    data rides in the wrapper in cleartext; the PayCoin currency is verified
//!    online against the simulated balance changes in `/submit`. See
//!    `reconstruct_operations`.
//!
//! ## Why proto rather than BCS for the inner transaction
//!
//! BCS is positional; any change to `TransactionData`'s layout is a wire
//! break for every consumer holding bytes built before the change. The
//! proto representation is tag-based, so adding optional fields in future
//! versions is non-breaking on the wire even though prost-generated
//! decoders here do not retain unknown fields across decode/encode. The
//! forward-compatibility benefit is on the wire format itself, not on the
//! prost runtime — concretely, an older binary handling a newer-shape
//! wrapper will silently drop fields it doesn't know about rather than
//! hard-fail at decode like BCS would.

use fastcrypto::encoding::Hex;
use prost::Message;
use serde::{Deserialize, Serialize};
use sui_rpc::proto::sui::rpc::v2::Transaction as ProtoTransaction;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;

use crate::Currency;
use crate::errors::Error;
use crate::types::RedeemMode;

/// The only Rosetta-level labels `/parse` cannot reconstruct from the proto
/// `Transaction`. Each non-`None` variant corresponds to an operation family
/// carrying a label that lives in chain state rather than the PTB.
///
/// `/metadata` populates this and it rides in the wrapper in cleartext. The
/// PayCoin `currency` — the one label that affects fund routing — is verified
/// online against the simulated balance changes in `/submit`; the FSS labels
/// are display-only (the signed PTB determines execution).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum AuxData {
    /// PaySui, Stake, WithdrawStake: fully reconstructable from the PTB.
    None,
    /// PayCoin: the moved coin type is not in the PTB (only an `ObjectRef` →
    /// chain state). `currency` also disambiguates PayCoin from PaySui at parse
    /// time.
    PayCoin { currency: Currency },
    /// ConsolidateAllStakedSuiToFungible: `validator` is derived from a pool id
    /// via chain-state lookup and is not recoverable from the PTB. Object ids
    /// ARE recoverable from PTB inputs, so they are not carried here.
    Consolidate { validator: SuiAddress },
    /// MergeAndRedeemFungibleStakedSui: `validator` is not recoverable; the
    /// AtMost classification + cap are not byte-encodable. All / AtLeast are
    /// recoverable, but we carry the mode uniformly so `/parse` reports it.
    MergeAndRedeem {
        validator: SuiAddress,
        redeem_mode: RedeemMode,
        amount: Option<u64>,
    },
}

/// The rosetta construction-flow wrapper, used for both unsigned (`/payloads`
/// output) and signed (`/combine` output) transactions. The two states differ
/// only by whether `signatures` is populated, so a single type carries both;
/// `signatures.is_empty()` means unsigned.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RosettaTransaction {
    /// hex(prost::encode(proto::Transaction)). The proto's structured fields
    /// (`kind`, `sender`, `gas_payment`, `expiration`) are populated; the
    /// opaque `bcs` field is cleared.
    pub transaction: Hex,
    /// `GenericSignature` bytes (flag + sig + pubkey), hex-encoded, one per
    /// signer. Empty for an unsigned transaction; populated by `/combine`.
    /// Carried alongside `transaction` rather than embedded in it, matching the
    /// gRPC `ExecuteTransactionRequest::with_signatures(...)` shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<Hex>,
    /// Minimal Rosetta-level labels the PTB cannot encode (cleartext).
    pub aux: AuxData,
}

/// Build the wire form of `TransactionData` for the wrapper: convert to the
/// proto Transaction via the existing `From<TransactionData>` impl in
/// `sui_types::rpc_proto_conversions`, then clear the opaque `bcs` field so
/// only the structured fields ride on the wire.
pub fn encode_inner_proto(data: &TransactionData) -> Hex {
    let mut proto: ProtoTransaction = data.clone().into();
    proto.bcs = None;
    Hex::from_bytes(&proto.encode_to_vec())
}

pub fn decode_inner_proto(hex: &Hex) -> Result<ProtoTransaction, Error> {
    let bytes = hex.to_vec()?;
    let proto = ProtoTransaction::decode(bytes.as_slice())
        .map_err(|e| Error::DataError(format!("decode inner proto: {e}")))?;
    // Reject any envelope whose inner proto carries the opaque `bcs` field.
    // `encode_inner_proto` always clears it; a populated `bcs` here means the
    // wrapper came from somewhere else. The SDK→TransactionData conversion in
    // `proto_to_transaction_data` reads structured fields, but downstream gRPC
    // paths (including the validator's simulate handler) prefer `bcs` when
    // present — so a wrapper with benign structured fields and a divergent
    // `bcs` would let `/parse` report what the structured fields decode to
    // while `/submit` broadcasts whatever the `bcs` blob decodes to.
    if proto.bcs.is_some() {
        return Err(Error::DataError(
            "envelope inconsistency: inner proto carries a populated `bcs` field; rosetta-built \
             envelopes have `bcs` cleared"
                .to_string(),
        ));
    }
    Ok(proto)
}

/// Reverse of `encode_inner_proto`. Mirrors the path used by
/// `sui-rpc-api/.../simulate/mod.rs`: proto → `sui_sdk_types::Transaction`
/// (reads structured fields) → `TransactionData` (via the BCS-round-trip impl
/// in `sui_types::sui_sdk_types_conversions`).
pub fn proto_to_transaction_data(proto: ProtoTransaction) -> Result<TransactionData, Error> {
    let sdk_tx = sui_sdk_types::Transaction::try_from(&proto)
        .map_err(|e| Error::DataError(format!("proto → sdk transaction: {e}")))?;
    TransactionData::try_from(sdk_tx)
        .map_err(|e| Error::DataError(format!("sdk transaction → TransactionData: {e}")))
}

pub fn encode(w: &RosettaTransaction) -> Result<Hex, Error> {
    let bytes = serde_json::to_vec(w)
        .map_err(|e| Error::DataError(format!("serialize transaction wrapper: {e}")))?;
    Ok(Hex::from_bytes(&bytes))
}

pub fn decode(hex: &Hex) -> Result<RosettaTransaction, Error> {
    let bytes = hex.to_vec()?;
    let w: RosettaTransaction = serde_json::from_slice(&bytes)
        .map_err(|e| Error::DataError(format!("decode transaction wrapper: {e}")))?;
    // Validate any signatures present (a no-op for an unsigned wrapper): reject
    // blobs that can't parse as a `GenericSignature` (`flag || sig || pubkey`).
    // `/hash` and `/submit` decode these too, but `/parse` previously passed
    // through without touching them — surfacing structural garbage here means a
    // signed `/parse` request actually acts as a sanity check on the signed bytes.
    for (i, sig_hex) in w.signatures.iter().enumerate() {
        let raw = sig_hex.to_vec().map_err(|e| {
            Error::DataError(format!(
                "signed wrapper signatures[{i}] is not valid hex: {e}"
            ))
        })?;
        GenericSignature::from_bytes(&raw).map_err(|e| {
            Error::DataError(format!(
                "signed wrapper signatures[{i}] does not parse as GenericSignature: {e}"
            ))
        })?;
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SUI;
    use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress};
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{TEST_ONLY_GAS_UNIT_FOR_TRANSFER, TransactionData};

    fn random_object_ref() -> ObjectRef {
        (
            ObjectID::random(),
            SequenceNumber::from(1),
            ObjectDigest::random(),
        )
    }

    /// Build a non-trivial PaySui `TransactionData` for round-trip tests.
    fn sample_pay_sui_data() -> TransactionData {
        let sender = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let pt = {
            let mut b = ProgrammableTransactionBuilder::new();
            b.pay_sui(vec![recipient], vec![1_000_000]).unwrap();
            b.finish()
        };
        let gas_price = 1000;
        TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        )
    }

    #[test]
    fn test_unsigned_wrapper_roundtrip() {
        let data = sample_pay_sui_data();
        let w = RosettaTransaction {
            transaction: encode_inner_proto(&data),
            signatures: vec![],
            aux: AuxData::PayCoin {
                currency: SUI.clone(),
            },
        };
        let hex = encode(&w).unwrap();
        let decoded = decode(&hex).unwrap();

        // Outer transaction bytes round-trip.
        assert_eq!(w.transaction, decoded.transaction);

        // Inner proto round-trips into structurally-equivalent TransactionData.
        let proto = decode_inner_proto(&decoded.transaction).unwrap();
        let recovered = proto_to_transaction_data(proto).unwrap();
        assert_eq!(data, recovered);
    }

    #[test]
    fn test_signed_wrapper_roundtrip() {
        let data = sample_pay_sui_data();
        let w = RosettaTransaction {
            transaction: encode_inner_proto(&data),
            // 96-byte sample: 1-byte flag + 64-byte sig + 32-byte pubkey
            signatures: vec![Hex::from_bytes(&[0u8; 97])],
            aux: AuxData::None,
        };
        let hex = encode(&w).unwrap();
        let decoded = decode(&hex).unwrap();
        assert_eq!(w.transaction, decoded.transaction);
        assert_eq!(w.signatures, decoded.signatures);
    }

    /// Run the wire round-trip (encode_inner_proto → decode → proto_to_data)
    /// and assert byte-equal BCS. This is the load-bearing invariant for
    /// digest stability: `/hash` proto-decodes the wrapper, converts to
    /// `TransactionData`, hashes BCS(IntentMessage<TransactionData>). If the
    /// round-trip ever produces non-canonical BCS, `/hash` would silently
    /// disagree with the validator-computed digest.
    ///
    /// This is NOT a prost serialize→deserialize round-trip (that part is safe
    /// by construction). It crosses three independently hand-maintained
    /// representations — `TransactionData` ↔ proto `Transaction` ↔
    /// `sui_sdk_types::Transaction` — through four hand-written conversions.
    /// Nothing compiler-checks that they agree, and they have drifted before:
    /// the `ValidDuring` ms-vs-seconds mismatch pinned by
    /// `test_ab_gas_valid_during_timestamps_are_none` is exactly a case where
    /// the encode and decode directions disagree. Two extra wrinkles specific
    /// to our usage: `encode_inner_proto` clears the opaque `bcs` blob, so this
    /// also proves the *structured* fields alone are lossless; and proto3
    /// elides default/empty values, so an absent-vs-zero mapping bug only
    /// surfaces on a fixture that actually carries the affected field.
    ///
    /// Covered per-variant because those conversions branch per structural
    /// element (`ImmOrOwnedObject` vs `SharedObject` CallArgs, `MoveCall` with
    /// vs without `type_arguments`, `Epoch` vs `ValidDuring` expiration,
    /// coin-gas vs address-balance gas). A bug in one branch is invisible
    /// unless a fixture's PTB exercises that branch — e.g. only the
    /// `merge_and_redeem_atleast` fixture carries MoveCall `type_arguments`
    /// (`balance::split<SUI>`), so it is the sole guard on that mapping.
    fn assert_canonical(data: &TransactionData, label: &str) {
        let proto_hex = encode_inner_proto(data);
        let proto = decode_inner_proto(&proto_hex).unwrap();
        let recovered = proto_to_transaction_data(proto).unwrap();
        assert_eq!(
            bcs::to_bytes(data).unwrap(),
            bcs::to_bytes(&recovered).unwrap(),
            "proto round-trip is not canonical for {label}"
        );
    }

    #[test]
    fn test_proto_to_bcs_canonicality_pay_sui() {
        assert_canonical(&sample_pay_sui_data(), "pay_sui");
    }

    /// §12 test 13: the digest `/hash` would return (decode signed wrapper →
    /// proto → `TransactionData` → `Transaction::digest()`) equals the digest of
    /// a `Transaction` built directly from the original `TransactionData`. This
    /// exercises the full `/hash` decode chain and pins digest stability across
    /// the wrapper round-trip — signatures do not affect the digest.
    #[test]
    fn test_hash_matches_inner_digest() {
        use sui_types::signature::GenericSignature;
        use sui_types::transaction::Transaction;

        let data = sample_pay_sui_data();
        // 97-byte ed25519-shaped GenericSignature (flag + 64 sig + 32 pubkey);
        // its contents don't affect the digest.
        let raw_sig = [0u8; 97];
        let sig = GenericSignature::from_bytes(&raw_sig).unwrap();
        let expected = *Transaction::from_generic_sig_data(data.clone(), vec![sig]).digest();

        let wrapper = RosettaTransaction {
            transaction: encode_inner_proto(&data),
            signatures: vec![Hex::from_bytes(&raw_sig)],
            aux: AuxData::None,
        };
        let decoded = decode(&encode(&wrapper).unwrap()).unwrap();
        let proto = decode_inner_proto(&decoded.transaction).unwrap();
        let recovered = proto_to_transaction_data(proto).unwrap();
        let sigs = decoded
            .signatures
            .iter()
            .map(|s| GenericSignature::from_bytes(&s.to_vec().unwrap()).unwrap())
            .collect::<Vec<_>>();
        let got = *Transaction::from_generic_sig_data(recovered, sigs).digest();

        assert_eq!(got, expected);
    }

    /// PayCoin path post-bearer-removal: same shape as PaySui but built via
    /// `pay_coin_pt`. Exercises the SplitCoins + per-recipient TransferObjects
    /// shape with a single input coin.
    #[test]
    fn test_proto_to_bcs_canonicality_pay_coin() {
        use crate::SUI;
        use crate::types::internal_operation::pay_coin_pt;

        let sender = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let coin = random_object_ref();
        let pt = pay_coin_pt(sender, vec![recipient], vec![10_000], &[coin], &[], 0, &SUI).unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "pay_coin");
    }

    /// PaySui with a party-owned (`SharedObject`) coin input. Exercises the
    /// `ObjectArg::SharedObject` proto path, distinct from `ImmOrOwnedObject`.
    #[test]
    fn test_proto_to_bcs_canonicality_pay_sui_party_object() {
        use crate::types::internal_operation::pay_sui_pt_coin_gas;

        let recipient = SuiAddress::random_for_testing_only();
        let sender = SuiAddress::random_for_testing_only();
        let party_coin = (ObjectID::random(), SequenceNumber::from(7));
        let pt = pay_sui_pt_coin_gas(vec![recipient], vec![5_000], &[], &[party_coin], 0).unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "pay_sui_party_object");
    }

    /// AB-gas flow: `new_programmable_with_address_balance_gas` uses
    /// `TransactionExpiration::ValidDuring` plus an empty gas-payment objects
    /// list and price=0. These are distinct proto-conversion paths from the
    /// standard `Epoch` expiration / coin-gas-payment shape.
    #[test]
    fn test_proto_to_bcs_canonicality_ab_gas_valid_during() {
        use crate::types::internal_operation::pay_sui_pt_ab_gas;
        use sui_types::digests::{ChainIdentifier, CheckpointDigest};

        let sender = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let pt = pay_sui_pt_ab_gas(sender, vec![recipient], vec![5_000], &[], &[], 5_000).unwrap();
        let chain_id = ChainIdentifier::from(CheckpointDigest::new([7u8; 32]));
        let data = TransactionData::new_programmable_with_address_balance_gas(
            sender,
            pt,
            0,
            1000,
            chain_id,
            42,
            0xdead_beef,
        );
        assert_canonical(&data, "ab_gas_valid_during");
    }

    /// §10.5 / §13.6 pin: rosetta-built address-balance-gas transactions must
    /// keep the `ValidDuring` `min/max_timestamp` fields `None`. The proto
    /// round-trip for those timestamps has a latent ms-vs-seconds mismatch
    /// (encode treats the value as ms via `ms_to_timestamp`, decode reads
    /// `.seconds`); it is inert only while both stay `None`. Do not start
    /// populating them — if this test ever needs changing, fix the conversion
    /// first.
    #[test]
    fn test_ab_gas_valid_during_timestamps_are_none() {
        use crate::types::internal_operation::pay_sui_pt_ab_gas;
        use sui_types::digests::{ChainIdentifier, CheckpointDigest};
        use sui_types::transaction::{TransactionDataAPI, TransactionExpiration};

        let sender = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let pt = pay_sui_pt_ab_gas(sender, vec![recipient], vec![5_000], &[], &[], 5_000).unwrap();
        let chain_id = ChainIdentifier::from(CheckpointDigest::new([7u8; 32]));
        let data = TransactionData::new_programmable_with_address_balance_gas(
            sender,
            pt,
            0,
            1000,
            chain_id,
            42,
            0xdead_beef,
        );
        match data.expiration() {
            TransactionExpiration::ValidDuring {
                min_timestamp,
                max_timestamp,
                ..
            } => {
                assert!(
                    min_timestamp.is_none() && max_timestamp.is_none(),
                    "ValidDuring timestamps must remain None (see §10.5)"
                );
            }
            other => panic!("expected ValidDuring expiration, got {other:?}"),
        }
    }

    /// Stake (system MoveCall): `MoveCall` + `SUI_SYSTEM_MUT` shared-object
    /// input + transfer-back-to-sender shapes.
    #[test]
    fn test_proto_to_bcs_canonicality_stake() {
        use crate::types::internal_operation::stake_pt_coin_gas;

        let sender = SuiAddress::random_for_testing_only();
        let validator = SuiAddress::random_for_testing_only();
        let coin = random_object_ref();
        let pt = stake_pt_coin_gas(validator, 1_000_000, false, &[coin], &[], 0).unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "stake");
    }

    /// WithdrawStake: shared-system-state input + per-stake `ImmOrOwnedObject`
    /// inputs + `request_withdraw_stake` MoveCall.
    #[test]
    fn test_proto_to_bcs_canonicality_withdraw_stake() {
        use crate::types::internal_operation::withdraw_stake_pt;

        let sender = SuiAddress::random_for_testing_only();
        let stake_obj = random_object_ref();
        let pt = withdraw_stake_pt(vec![stake_obj], false).unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "withdraw_stake");
    }

    /// Consolidate (FSS): multi-MoveCall PTB with object inputs of two
    /// different on-chain types.
    #[test]
    fn test_proto_to_bcs_canonicality_consolidate() {
        use crate::types::internal_operation::consolidate_to_fungible_pt;

        let sender = SuiAddress::random_for_testing_only();
        let pt = consolidate_to_fungible_pt(
            sender,
            vec![random_object_ref()],
            vec![random_object_ref()],
        )
        .unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "consolidate");
    }

    /// MergeAndRedeem with `AtLeast` plan + `balance::split` guard. Exercises
    /// the longest typed PTB rosetta builds.
    #[test]
    fn test_proto_to_bcs_canonicality_merge_and_redeem_atleast() {
        use crate::types::RedeemPlan;
        use crate::types::internal_operation::merge_and_redeem_fss_pt;

        let sender = SuiAddress::random_for_testing_only();
        let plan = RedeemPlan::AtLeast {
            token_amount: Some(500),
            min_sui: 1_000_000,
        };
        let pt = merge_and_redeem_fss_pt(
            sender,
            vec![random_object_ref(), random_object_ref()],
            &plan,
        )
        .unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "merge_and_redeem_atleast");
    }

    /// MergeAndRedeem `All` (no split, no guard). Different PTB shape from
    /// the `AtLeast` case.
    #[test]
    fn test_proto_to_bcs_canonicality_merge_and_redeem_all() {
        use crate::types::RedeemPlan;
        use crate::types::internal_operation::merge_and_redeem_fss_pt;

        let sender = SuiAddress::random_for_testing_only();
        let plan = RedeemPlan::All;
        let pt = merge_and_redeem_fss_pt(
            sender,
            vec![random_object_ref(), random_object_ref()],
            &plan,
        )
        .unwrap();
        let gas_price = 1000;
        let data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );
        assert_canonical(&data, "merge_and_redeem_all");
    }

    /// `decode` must reject signature blobs that aren't structurally valid
    /// `GenericSignature` bytes (`flag || sig || pubkey`). `/hash` and `/submit`
    /// do their own parsing downstream, but `/parse` for signed transactions
    /// previously passed signatures through unchecked. Catching garbage here
    /// makes `/parse(signed=true)` an actual sanity check on the signed bytes.
    #[test]
    fn test_decode_rejects_malformed_signature_bytes() {
        let data = sample_pay_sui_data();
        let w = RosettaTransaction {
            transaction: encode_inner_proto(&data),
            // Three bytes — too short to be a valid GenericSignature for any
            // supported scheme.
            signatures: vec![Hex::from_bytes(&[1, 2, 3])],
            aux: AuxData::None,
        };
        let bytes = serde_json::to_vec(&w).unwrap();
        let hex = Hex::from_bytes(&bytes);

        let err = decode(&hex).expect_err("malformed signature must be rejected");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("signatures") || msg.contains("GenericSignature"),
            "error should mention the signature; got: {msg}"
        );
    }

    /// `decode_inner_proto` must reject any wrapper whose inner proto has a
    /// populated `bcs` field. Downstream gRPC paths prefer `bcs` over the
    /// structured fields, so a crafted envelope with benign structured fields
    /// and a divergent `bcs` blob would let `/parse` report the structured
    /// fields while `/submit` broadcasts the `bcs` transaction. Rejecting at
    /// decode time closes that bypass.
    #[test]
    fn test_decode_inner_proto_rejects_populated_bcs() {
        use sui_rpc::proto::sui::rpc::v2::Bcs;
        // Build a valid proto, then re-attach a `bcs` field. The structured
        // fields stay valid; only the `bcs` presence is the violation.
        let data = sample_pay_sui_data();
        let mut proto: ProtoTransaction = data.into();
        proto.bcs = Some(Bcs::default());
        let hex = Hex::from_bytes(&proto.encode_to_vec());

        let err = decode_inner_proto(&hex).expect_err("populated bcs must be rejected");
        let msg = format!("{err:?}");
        assert!(msg.contains("bcs"), "error should mention bcs; got: {msg}");
    }
}

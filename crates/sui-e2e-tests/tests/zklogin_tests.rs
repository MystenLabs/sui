// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::AppId::Sui;
use crate::IntentScope::SenderSignedTransaction;
use crate::IntentVersion::V0;
use fastcrypto::bls12381::min_sig::{BLS12381AggregateSignature, BLS12381PublicKey};
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::traits::AggregateAuthenticator;
use fastcrypto::traits::ToFromBytes;
use shared_crypto::intent::{AppId, Intent, IntentMessage, IntentScope, IntentVersion};
use std::collections::BTreeSet;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::committee::EpochId;
use sui_types::crypto::Signable;
use sui_types::error::{SuiError, SuiResult};
use sui_types::signature::GenericSignature;
use sui_types::transaction::{CertifiedTransaction, SenderSignedData, Transaction};
use sui_types::utils::{get_zklogin_user_address, make_zklogin_tx, sign_zklogin_tx};
use test_cluster::TestClusterBuilder;

async fn do_zklogin_test() -> SuiResult {
    let test_cluster = TestClusterBuilder::new().build().await;
    let (_, tx, _) = make_zklogin_tx();

    test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .handle_transaction(tx)
        .await
        .map(|_| ())
}

#[test]
fn test_certs() {
    /* logs from ord-05
        Log 1 - good -
        {"timestamp":"2023-09-04T02:03:05.163128Z","level":"DEBUG","fields":{"message":"Collected tx certificate","ct":"Envelope { digest: OnceCell(TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)), data: SenderSignedData([SenderSignedTransaction { intent_message: IntentMessage { intent: Intent { scope: TransactionData, version: V0, app_id: Sui }, value: V1(TransactionDataV1 { kind: ProgrammableTransaction(ProgrammableTransaction { inputs: [Pure([160, 134, 1, 0, 0, 0, 0, 0]), Object(SharedObject { id: 0xdd7e3a071c6a090a157eccc3c9bbc4d2b3fb5ac9a4687b1c300bf74be6a58945, initial_shared_version: SequenceNumber(60), mutable: true }), Object(SharedObject { id: 0x0000000000000000000000000000000000000000000000000000000000000006, initial_shared_version: SequenceNumber(1), mutable: false }), Pure([160, 134, 1, 0, 0, 0, 0, 0]), Pure([48, 0, 0, 0, 0, 0, 0, 0])], commands: [SplitCoins(GasCoin, [Input(0)]), MoveCall(ProgrammableMoveCall { package: 0x88d362329ede856f5f67867929ed570bba06c975abec2fab7f0601c56f6a8cb1, module: Identifier(\"animeswap\"), function: Identifier(\"swap_exact_coins_for_coins_2_pair_entry\"), type_arguments: [Struct(StructTag { address: 0000000000000000000000000000000000000000000000000000000000000002, module: Identifier(\"sui\"), name: Identifier(\"SUI\"), type_params: [] }), Struct(StructTag { address: 5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf, module: Identifier(\"coin\"), name: Identifier(\"COIN\"), type_params: [] }), Struct(StructTag { address: c060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c, module: Identifier(\"coin\"), name: Identifier(\"COIN\"), type_params: [] })], arguments: [Input(1), Input(2), Result(0), Input(3), Input(4)] })] }), sender: 0x50fdf1f6e7c3f9c4a2dc20a6ca9bc466b3d1d45989780a0fea44e46eef84704c, gas_data: GasData { payment: [(0x554a43559e1fa91dde2d5d41b1c80096df0682abe87048e1b02501a3c5c62f62, SequenceNumber(27195602), o#FivWLUYYoyLWWMJ7C1cVeCTknJYSzyH29SUrWenpvegS)], owner: 0x50fdf1f6e7c3f9c4a2dc20a6ca9bc466b3d1d45989780a0fea44e46eef84704c, price: 755, budget: 100000000 }, expiration: None }) }, tx_signatures: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 188, 97, 157, 27, 136, 127, 201, 52, 211, 142, 220, 20, 30, 243, 17, 111, 39, 246, 105, 63, 90, 229, 178, 125, 73, 61, 221, 241, 201, 140, 130, 3, 63, 199, 59, 37, 245, 224, 174, 159, 14, 178, 168, 0, 46, 91, 38, 156, 234, 55, 234, 121, 81, 52, 129, 124, 247, 104, 2, 177, 60, 110, 124, 7, 245, 237, 148, 230, 187, 91, 98, 40, 226, 203, 232, 42, 38, 185, 145, 230, 225, 123, 89, 2, 218, 204, 76, 123, 17, 220, 239, 26, 81, 33, 15, 80])))] }]), auth_signature: AuthorityQuorumSignInfo { epoch: 144, signature: BLS12381AggregateSignature { sig: Signature { point: blst_p1_affine { x: blst_fp { l: [13307527071167199373, 942000580494760865, 3868300817856580772, 15448281516218267282, 11107363828734407678, 677183325569699376] }, y: blst_fp { l: [11568656431264391262, 6550321605706497373, 3353259001722580085, 14149557188465194444, 5348232613704608523, 746863603985800712] } } }, bytes: OnceCell([163, 197, 138, 68, 49, 69, 35, 132, 48, 169, 213, 238, 95, 84, 170, 149, 249, 154, 87, 216, 155, 10, 54, 202, 126, 200, 37, 163, 54, 164, 98, 255, 27, 226, 94, 226, 182, 56, 172, 53, 52, 122, 246, 26, 88, 116, 140, 36]) }, signers_map: RoaringBitmap<64 values between 2 and 103> } }","ct_bytes":"[1, 0, 0, 0, 0, 0, 5, 0, 8, 160, 134, 1, 0, 0, 0, 0, 0, 1, 1, 221, 126, 58, 7, 28, 106, 9, 10, 21, 126, 204, 195, 201, 187, 196, 210, 179, 251, 90, 201, 164, 104, 123, 28, 48, 11, 247, 75, 230, 165, 137, 69, 60, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 160, 134, 1, 0, 0, 0, 0, 0, 0, 8, 48, 0, 0, 0, 0, 0, 0, 0, 2, 2, 0, 1, 1, 0, 0, 0, 136, 211, 98, 50, 158, 222, 133, 111, 95, 103, 134, 121, 41, 237, 87, 11, 186, 6, 201, 117, 171, 236, 47, 171, 127, 6, 1, 197, 111, 106, 140, 177, 9, 97, 110, 105, 109, 101, 115, 119, 97, 112, 39, 115, 119, 97, 112, 95, 101, 120, 97, 99, 116, 95, 99, 111, 105, 110, 115, 95, 102, 111, 114, 95, 99, 111, 105, 110, 115, 95, 50, 95, 112, 97, 105, 114, 95, 101, 110, 116, 114, 121, 3, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 115, 117, 105, 3, 83, 85, 73, 0, 7, 93, 75, 48, 37, 6, 100, 92, 55, 255, 19, 59, 152, 196, 181, 10, 90, 225, 72, 65, 101, 151, 56, 214, 215, 51, 213, 157, 13, 33, 122, 147, 191, 4, 99, 111, 105, 110, 4, 67, 79, 73, 78, 0, 7, 192, 96, 0, 97, 17, 1, 107, 138, 2, 10, 213, 179, 56, 52, 152, 74, 67, 122, 170, 125, 60, 116, 193, 142, 9, 169, 93, 72, 172, 234, 176, 140, 4, 99, 111, 105, 110, 4, 67, 79, 73, 78, 0, 5, 1, 1, 0, 1, 2, 0, 2, 0, 0, 1, 3, 0, 1, 4, 0, 80, 253, 241, 246, 231, 195, 249, 196, 162, 220, 32, 166, 202, 155, 196, 102, 179, 209, 212, 89, 137, 120, 10, 15, 234, 68, 228, 110, 239, 132, 112, 76, 1, 85, 74, 67, 85, 158, 31, 169, 29, 222, 45, 93, 65, 177, 200, 0, 150, 223, 6, 130, 171, 232, 112, 72, 225, 176, 37, 1, 163, 197, 198, 47, 98, 210, 248, 158, 1, 0, 0, 0, 0, 32, 218, 192, 237, 232, 141, 208, 197, 52, 127, 85, 133, 224, 92, 147, 87, 23, 5, 174, 223, 80, 26, 50, 236, 245, 177, 122, 139, 15, 15, 37, 71, 59, 80, 253, 241, 246, 231, 195, 249, 196, 162, 220, 32, 166, 202, 155, 196, 102, 179, 209, 212, 89, 137, 120, 10, 15, 234, 68, 228, 110, 239, 132, 112, 76, 243, 2, 0, 0, 0, 0, 0, 0, 0, 225, 245, 5, 0, 0, 0, 0, 0, 1, 97, 0, 188, 97, 157, 27, 136, 127, 201, 52, 211, 142, 220, 20, 30, 243, 17, 111, 39, 246, 105, 63, 90, 229, 178, 125, 73, 61, 221, 241, 201, 140, 130, 3, 63, 199, 59, 37, 245, 224, 174, 159, 14, 178, 168, 0, 46, 91, 38, 156, 234, 55, 234, 121, 81, 52, 129, 124, 247, 104, 2, 177, 60, 110, 124, 7, 245, 237, 148, 230, 187, 91, 98, 40, 226, 203, 232, 42, 38, 185, 145, 230, 225, 123, 89, 2, 218, 204, 76, 123, 17, 220, 239, 26, 81, 33, 15, 80, 144, 0, 0, 0, 0, 0, 0, 0, 163, 197, 138, 68, 49, 69, 35, 132, 48, 169, 213, 238, 95, 84, 170, 149, 249, 154, 87, 216, 155, 10, 54, 202, 126, 200, 37, 163, 54, 164, 98, 255, 27, 226, 94, 226, 182, 56, 172, 53, 52, 122, 246, 26, 88, 116, 140, 36, 144, 1, 58, 48, 0, 0, 1, 0, 0, 0, 0, 0, 63, 0, 16, 0, 0, 0, 2, 0, 3, 0, 4, 0, 5, 0, 8, 0, 9, 0, 15, 0, 17, 0, 18, 0, 19, 0, 22, 0, 23, 0, 26, 0, 28, 0, 29, 0, 30, 0, 33, 0, 34, 0, 35, 0, 37, 0, 38, 0, 40, 0, 44, 0, 45, 0, 46, 0, 47, 0, 48, 0, 50, 0, 51, 0, 54, 0, 55, 0, 56, 0, 57, 0, 58, 0, 59, 0, 60, 0, 61, 0, 62, 0, 63, 0, 65, 0, 66, 0, 68, 0, 69, 0, 71, 0, 74, 0, 75, 0, 76, 0, 79, 0, 80, 0, 83, 0, 84, 0, 85, 0, 87, 0, 89, 0, 90, 0, 92, 0, 94, 0, 95, 0, 97, 0, 98, 0, 100, 0, 101, 0, 102, 0, 103, 0]","ct_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)"},"target":"sui_core::authority_aggregator","filename":"crates/sui-core/src/authority_aggregator.rs","line_number":1411,"span":{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"},"spans":[{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"}]}

        Log 2 - bad -
        {"timestamp":"2023-09-04T02:07:51.305898Z","level":"WARN","fields":{"message":"Failed to batch verify aggregated auth sig: General cryptographic error (chunk 0): pks: [[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"},"target":"sui_types::crypto","filename":"crates/sui-types/src/crypto.rs","line_number":1581,"span":{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"},"spans":[{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"}]}
        {"timestamp":"2023-09-04T02:07:51.307289Z","level":"WARN","fields":{"message":"Failed to batch verify aggregated auth sig: General cryptographic error (chunk 1): ogkoYCt+o9Z7EiqIcYAKsQ3ahpCX3qXaHGjdw, kv2vunhTvcCZ5ulaNTpTLIkL9ZL8OH1567D6IObk2C+b+1odE9QZgQhedcs9e61kEiDJTyuVmpH9ja93lpjbHXxWh/CZQy7w7pdykoXt1CA7S+6dG2xcV3+PQEAoR2k6, k35fl1B8tHZDQd+zgXcmAry7be/Ljoj8RNjiF78nXhwcsOXEdvjUwPlxzQGKOiF9Evei4KvIxuJaktVOD/y5MBTznsQ38w46RoLWZRjB35zlD+V14u4d/lswa394o2cX, lHO2S8bIwDOZG0wb15ZcJ41cz/OHncllxax5EZhDGKhE5b8huESlkTV9w3dz+I/UCGfPCQ3wfoEBR0QVcUZ2ge/uewnwMT8GSzDNTDy4K8C2f7B9jDo9WBMywaXCMwJA, lRKmfqRcMrsuZei7zPYH+25TQDgKvisFL68z3whqAsJmykTzX2gZbBfYwFlBPKKwATkEZFzcLFY8AnyQaf709WO9Z+jdbBMxW4klFT1K2suHoS9E9T+wPyyJIeGojib5, la/0YtYAaz3u1kwhqUOTxeG5Zgvn2T1sneIs8aokiDoprp/ng0A/BodaHfaUMS36AFfvcs0n0e8iiakFmHre2GgUh/aTYthRV+Wp4oO2PQEBl0HssIEBZzVhYR9L/06m, ljwIccgQQZFMUx+l6LbO0hgDfag4wtJk/voiLMKlHNEfVi2dE8/L5I428qabeWPWDrWG4fbvvd14rmXCqKOSy9T5hPCqHN/LEU72umjaL7qeiwmId/4yqCLCAz3IAJQF, lqoAlMzKNJceoBqPBEVxQ5YFoV2kmnsdcR+gPXWqYU/Fp/upgaxV9fM8xwD+XWU6EJuJPvTX+dgd/TWqoNQm9QCxqM0uTPHOmxrLMZoPwfAPTgqYvWC4aZtZ5Y3KfFEl, lt/bI5DNmWCk6vFmyEZueTaFl/YDug4nNpbyykaqXA6T9bOVJ9utYKba3QF6eP60Gf+AphJc2fvDXnbnls0sdqDNm25VNyarONHil7wWNsh9pujLhdXkmz1IKDmKLk85, l5ex+OuRHLzXhjsHTmtxKSzK9i0/2s+n0fSqzKFeVnpRjLURnReDtEDMriGGhuotBN19CeRnqK6U/i6Y25jshJPOfYD2awD9xseg4DfG9Dv8GEjmPXJhmvmiruTJoPxr, l9DQWw/rH0opQ89+Gareh9gywnQwrOBbYUI3hrCHg7Xu4PURJM1dKMNfbPOBkrCyD519kkD0JJXm0uZdkYFuhUv7Lw4vSm5mPNlEEIZbDedYg/2zrRoRVTiMB5azYvGD, mcs35aJ2Zn0ZVXlEDFpAU8n7bpFyE8mi9FfrDPRe0DpJu06OEy8vMTcBhj4QbMn2EzzHoXHKhRTjRwzBFfd6s+3/9qpJFJ8ON4U74uWUvLtH/sShjNqrZ1Dbk8RdBMC0, meTHQVEnxe8CXg/T0ZQfyajfd65jYIVcuWV4IKHW2q4rxcczOmd0J2B9hvINKY/uF8JCzUzdGci8qpmfv0uiOkmC2UQcGSIYRajqXPiPotxjaDqujBlPxIas4oY2K981, oXwL5C63LwG+HeFg3/2PYQhlnZ+G0HlWJxihQYUENMnyk9MxvBBBGwntUlifmV6oA/ho6eASqxq4yQiIdht7Hehf2Nxgb1IXHJmh7+w5Xxo7t1TGxKLc/SAaK1wJQBv2, o+XXM4yg5QAK+Nt7xJAp6g03bPztUGa9ykQRI5vC/h1o6Wy8gTO19vqGROzmXOWACGBFVx/edWGBAogzImLmXk9IQKjZ2cLoglbt1TTIgaVOvX3cMlkaM2CELdvwuYtx, o/Dc0BRcRE1GWdmmsb2ItUILgPapRKpfHbWBVIWyU6AuOj1XpdALGJKu7GPjyJE5CdaCuuK3Bib37da5GDpnVgj3VtjXIQFWgi6eocg7l4vnKFtbsd8EFMzjd0A2O96f, pAcp4TSgj667rdrWNAj5HcXbPcCns0MXpI6vLddIfJI2arg04mprsn+0mqu"},"target":"sui_types::crypto","filename":"crates/sui-types/src/crypto.rs","line_number":1581,"span":{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"},"spans":[{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"}]}
        {"timestamp":"2023-09-04T02:07:51.307295Z","level":"WARN","fields":{"message":"Failed to batch verify aggregated auth sig: General cryptographic error (chunk 2): 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, sIKADwnYVGi7T2itDiynaVpFzUk"},"target":"sui_types::crypto","filename":"crates/sui-types/src/crypto.rs","line_number":1581,"span":{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"},"spans":[{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"}]}
        {"timestamp":"2023-09-04T02:07:51.318773Z","level":"WARN","fields":{"message":"Failed to batch verify aggregated auth sig: General cryptographic error (chunk 3): 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"},"target":"sui_types::crypto","filename":"crates/sui-types/src/crypto.rs","line_number":1581,"span":{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"},"spans":[{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"}]}
        {"timestamp":"2023-09-04T02:07:51.319201Z","level":"WARN","fields":{"message":"Failed to batch verify aggregated auth sig: General cryptographic error (chunk 4): DTD, uIbO5mPWaBAgWGjP2fRF948F/TSD/iI0q/T/14PAdfo3DNIKJS46Y+1SCuVjNZjOAB3DHFbYYlUfbHgjRrJvcx7USy8rCp/BH5QGBxZLcH3/83GtlO+Z8hk+ONmpdBGb]], messages: [\"BAAAAQAAAAAABQAIoIYBAAAAAAABAd1+OgccagkKFX7Mw8m7xNKz+1rJpGh7HDAL90vmpYlFPAAAAAAAAAABAQEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABgEAAAAAAAAAAAAIoIYBAAAAAAAACDAAAAAAAAAAAgIAAQEAAACI02Iynt6Fb19nhnkp7VcLugbJdavsL6t/BgHFb2qMsQlhbmltZXN3YXAnc3dhcF9leGFjdF9jb2luc19mb3JfY29pbnNfMl9wYWlyX2VudHJ5AwcAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgNzdWkDU1VJAAddSzAlBmRcN/8TO5jEtQpa4UhBZZc41tcz1Z0NIXqTvwRjb2luBENPSU4AB8BgAGERAWuKAgrVszg0mEpDeqp9PHTBjgmpXUis6rCMBGNvaW4EQ09JTgAFAQEAAQIAAgAAAQMAAQQAUP3x9ufD+cSi3CCmypvEZrPR1FmJeAoP6kTkbu+EcEwBVUpDVZ4fqR3eLV1BscgAlt8GgqvocEjhsCUBo8XGL2LS+J4BAAAAACDawO3ojdDFNH9VheBck1cXBa7fUBoy7PWxeosPDyVHO1D98fbnw/nEotwgpsqbxGaz0dRZiXgKD+pE5G7vhHBM8wIAAAAAAAAA4fUFAAAAAAABYQAJbkZR8GNYvYhx+j4kjCCJO5z6t0heoaoS+T/7CahDr4Fg5kv5Zw3VI/saJHu29tW4V0L3O0os4pwwZmlzfDkL9e2U5rtbYijiy+gqJrmR5uF7WQLazEx7EdzvGlEhD1CQAAAAAAAAAA==\"], sigs: [\"o8WKRDFFI4QwqdXuX1SqlfmaV9ibCjbKfsglozakYv8b4l7itjisNTR69hpYdIwk\"]"},"target":"sui_types::crypto","filename":"crates/sui-types/src/crypto.rs","line_number":1581,"span":{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"},"spans":[{"tx_digest":"TransactionDigest(H7tQjifkU2cBo3NqhF4Y751C4cw7LsQV1rk8RhtXT43R)","name":"aggregator_process_tx"}]}
    */

    // PKs are taken from the second log
    let pks = [
        "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",

    ]
        .iter()
        .map(|s| BLS12381PublicKey::from_bytes(&Base64::decode(s).unwrap()).unwrap())
        .collect::<Vec<_>>();

    // Cert from first log (ct_bytes)
    let cert_bytes = [
        1, 0, 0, 0, 0, 0, 5, 0, 8, 160, 134, 1, 0, 0, 0, 0, 0, 1, 1, 221, 126, 58, 7, 28, 106, 9,
        10, 21, 126, 204, 195, 201, 187, 196, 210, 179, 251, 90, 201, 164, 104, 123, 28, 48, 11,
        247, 75, 230, 165, 137, 69, 60, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 1, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 8, 160, 134, 1, 0, 0, 0, 0, 0, 0, 8, 48, 0, 0, 0, 0, 0, 0, 0, 2, 2, 0, 1, 1, 0, 0, 0,
        136, 211, 98, 50, 158, 222, 133, 111, 95, 103, 134, 121, 41, 237, 87, 11, 186, 6, 201, 117,
        171, 236, 47, 171, 127, 6, 1, 197, 111, 106, 140, 177, 9, 97, 110, 105, 109, 101, 115, 119,
        97, 112, 39, 115, 119, 97, 112, 95, 101, 120, 97, 99, 116, 95, 99, 111, 105, 110, 115, 95,
        102, 111, 114, 95, 99, 111, 105, 110, 115, 95, 50, 95, 112, 97, 105, 114, 95, 101, 110,
        116, 114, 121, 3, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 115, 117, 105, 3, 83, 85, 73, 0, 7, 93, 75, 48, 37, 6, 100,
        92, 55, 255, 19, 59, 152, 196, 181, 10, 90, 225, 72, 65, 101, 151, 56, 214, 215, 51, 213,
        157, 13, 33, 122, 147, 191, 4, 99, 111, 105, 110, 4, 67, 79, 73, 78, 0, 7, 192, 96, 0, 97,
        17, 1, 107, 138, 2, 10, 213, 179, 56, 52, 152, 74, 67, 122, 170, 125, 60, 116, 193, 142, 9,
        169, 93, 72, 172, 234, 176, 140, 4, 99, 111, 105, 110, 4, 67, 79, 73, 78, 0, 5, 1, 1, 0, 1,
        2, 0, 2, 0, 0, 1, 3, 0, 1, 4, 0, 80, 253, 241, 246, 231, 195, 249, 196, 162, 220, 32, 166,
        202, 155, 196, 102, 179, 209, 212, 89, 137, 120, 10, 15, 234, 68, 228, 110, 239, 132, 112,
        76, 1, 85, 74, 67, 85, 158, 31, 169, 29, 222, 45, 93, 65, 177, 200, 0, 150, 223, 6, 130,
        171, 232, 112, 72, 225, 176, 37, 1, 163, 197, 198, 47, 98, 210, 248, 158, 1, 0, 0, 0, 0,
        32, 218, 192, 237, 232, 141, 208, 197, 52, 127, 85, 133, 224, 92, 147, 87, 23, 5, 174, 223,
        80, 26, 50, 236, 245, 177, 122, 139, 15, 15, 37, 71, 59, 80, 253, 241, 246, 231, 195, 249,
        196, 162, 220, 32, 166, 202, 155, 196, 102, 179, 209, 212, 89, 137, 120, 10, 15, 234, 68,
        228, 110, 239, 132, 112, 76, 243, 2, 0, 0, 0, 0, 0, 0, 0, 225, 245, 5, 0, 0, 0, 0, 0, 1,
        97, 0, 188, 97, 157, 27, 136, 127, 201, 52, 211, 142, 220, 20, 30, 243, 17, 111, 39, 246,
        105, 63, 90, 229, 178, 125, 73, 61, 221, 241, 201, 140, 130, 3, 63, 199, 59, 37, 245, 224,
        174, 159, 14, 178, 168, 0, 46, 91, 38, 156, 234, 55, 234, 121, 81, 52, 129, 124, 247, 104,
        2, 177, 60, 110, 124, 7, 245, 237, 148, 230, 187, 91, 98, 40, 226, 203, 232, 42, 38, 185,
        145, 230, 225, 123, 89, 2, 218, 204, 76, 123, 17, 220, 239, 26, 81, 33, 15, 80, 144, 0, 0,
        0, 0, 0, 0, 0, 163, 197, 138, 68, 49, 69, 35, 132, 48, 169, 213, 238, 95, 84, 170, 149,
        249, 154, 87, 216, 155, 10, 54, 202, 126, 200, 37, 163, 54, 164, 98, 255, 27, 226, 94, 226,
        182, 56, 172, 53, 52, 122, 246, 26, 88, 116, 140, 36, 144, 1, 58, 48, 0, 0, 1, 0, 0, 0, 0,
        0, 63, 0, 16, 0, 0, 0, 2, 0, 3, 0, 4, 0, 5, 0, 8, 0, 9, 0, 15, 0, 17, 0, 18, 0, 19, 0, 22,
        0, 23, 0, 26, 0, 28, 0, 29, 0, 30, 0, 33, 0, 34, 0, 35, 0, 37, 0, 38, 0, 40, 0, 44, 0, 45,
        0, 46, 0, 47, 0, 48, 0, 50, 0, 51, 0, 54, 0, 55, 0, 56, 0, 57, 0, 58, 0, 59, 0, 60, 0, 61,
        0, 62, 0, 63, 0, 65, 0, 66, 0, 68, 0, 69, 0, 71, 0, 74, 0, 75, 0, 76, 0, 79, 0, 80, 0, 83,
        0, 84, 0, 85, 0, 87, 0, 89, 0, 90, 0, 92, 0, 94, 0, 95, 0, 97, 0, 98, 0, 100, 0, 101, 0,
        102, 0, 103, 0,
    ];
    let ct: CertifiedTransaction = bcs::from_bytes(&cert_bytes).unwrap();
    println!("ct: {:?}", ct);
    println!(
        "ct bls sig: {:?}",
        Base64::encode(ct.auth_sig().signature.as_ref())
    ); // was o8WKRDFFI4QwqdXuX1SqlfmaV9ibCjbKfsglozakYv8b4l7itjisNTR69hpYdIwk in the second log
    let tx1 = ct.data();
    let tx1_intent = IntentMessage::new(
        Intent {
            scope: SenderSignedTransaction,
            version: V0,
            app_id: Sui,
        },
        tx1,
    );
    println!("tx1_intent: {:?}", tx1_intent);
    let mut msg1 = bcs::to_bytes(&tx1_intent).expect("Message serialization should not fail");
    let epoch: EpochId = 144;
    epoch.write(&mut msg1);
    println!("msg1 for signing: {:?}", Base64::encode(&msg1));

    // From second log
    let msg2 = Base64::decode("BAAAAQAAAAAABQAIoIYBAAAAAAABAd1+OgccagkKFX7Mw8m7xNKz+1rJpGh7HDAL90vmpYlFPAAAAAAAAAABAQEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABgEAAAAAAAAAAAAIoIYBAAAAAAAACDAAAAAAAAAAAgIAAQEAAACI02Iynt6Fb19nhnkp7VcLugbJdavsL6t/BgHFb2qMsQlhbmltZXN3YXAnc3dhcF9leGFjdF9jb2luc19mb3JfY29pbnNfMl9wYWlyX2VudHJ5AwcAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgNzdWkDU1VJAAddSzAlBmRcN/8TO5jEtQpa4UhBZZc41tcz1Z0NIXqTvwRjb2luBENPSU4AB8BgAGERAWuKAgrVszg0mEpDeqp9PHTBjgmpXUis6rCMBGNvaW4EQ09JTgAFAQEAAQIAAgAAAQMAAQQAUP3x9ufD+cSi3CCmypvEZrPR1FmJeAoP6kTkbu+EcEwBVUpDVZ4fqR3eLV1BscgAlt8GgqvocEjhsCUBo8XGL2LS+J4BAAAAACDawO3ojdDFNH9VheBck1cXBa7fUBoy7PWxeosPDyVHO1D98fbnw/nEotwgpsqbxGaz0dRZiXgKD+pE5G7vhHBM8wIAAAAAAAAA4fUFAAAAAAABYQAJbkZR8GNYvYhx+j4kjCCJO5z6t0heoaoS+T/7CahDr4Fg5kv5Zw3VI/saJHu29tW4V0L3O0os4pwwZmlzfDkL9e2U5rtbYijiy+gqJrmR5uF7WQLazEx7EdzvGlEhD1CQAAAAAAAAAA==").unwrap();
    println!("msg2 for signing: {:?}", Base64::encode(&msg2));
    let epoch: EpochId = bcs::from_bytes(&msg2[msg2.len() - 8..]).unwrap();
    println!("epoch from second log: {:?}", epoch);

    let tx2_intent: IntentMessage<SenderSignedData> =
        bcs::from_bytes(&msg2[..msg2.len() - 8]).unwrap();
    println!("tx2_intent: {:?}", tx2_intent);

    // The difference is in the user sigs
    println!("msg1 sig: {:?}", tx1_intent.value.tx_signatures()[0]);
    println!("msg2 sig: {:?}", tx2_intent.value.tx_signatures()[0]);

    let sig = BLS12381AggregateSignature::from_bytes(
        &Base64::decode("o8WKRDFFI4QwqdXuX1SqlfmaV9ibCjbKfsglozakYv8b4l7itjisNTR69hpYdIwk")
            .unwrap(),
    )
    .unwrap();

    // same sig and pks work with msg1 but not with msg2
    assert!(BLS12381AggregateSignature::batch_verify(&[&sig], vec![pks.iter()], &[&msg1]).is_ok());
    assert!(BLS12381AggregateSignature::batch_verify(&[&sig], vec![pks.iter()], &[&msg2]).is_err());
}

#[sim_test]
async fn test_zklogin_feature_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_for_testing(false);
        config
    });

    let err = do_zklogin_test().await.unwrap_err();

    assert!(matches!(err, SuiError::UnsupportedFeatureError { .. }));
}

#[ignore("re-enable after JWK management is finished")]
#[sim_test]
async fn test_zklogin_provider_not_supported() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_for_testing(true);
        config.set_enable_jwk_consensus_updates_for_testing(true);
        config.set_zklogin_supported_providers(BTreeSet::from([
            "Google".to_string(),
            "Facebook".to_string(),
        ]));
        config
    });

    // Doing a Twitch zklogin tx fails because its not in the supported list.
    let err = do_zklogin_test().await.unwrap_err();

    assert!(matches!(err, SuiError::InvalidSignature { .. }));
}

#[ignore("re-enable after JWK management is finished")]
#[sim_test]
async fn test_zklogin_feature_allow() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_for_testing(true);
        config.set_enable_jwk_consensus_updates_for_testing(true);
        config.set_zklogin_supported_providers(BTreeSet::from(["Twitch".to_string()]));
        config
    });

    let err = do_zklogin_test().await.unwrap_err();

    // we didn't make a real transaction with a valid object, but we verify that we pass the
    // feature gate.
    assert!(matches!(err, SuiError::UserInputError { .. }));
}

#[ignore("re-enable after JWK management is finished")]
#[sim_test]
async fn zklogin_end_to_end_test() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_for_testing(true);
        config.set_enable_jwk_consensus_updates_for_testing(true);
        config
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let sender = test_cluster.get_address_0();

    let context = &mut test_cluster.wallet;
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let gas_object = accounts_and_objs[0].1[0];
    let object_to_send = accounts_and_objs[0].1[1];

    let zklogin_addr = get_zklogin_user_address();

    // first send an object to the zklogin address.
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, rgp)
            .transfer(object_to_send, zklogin_addr)
            .build(),
    );

    context.execute_transaction_must_succeed(txn).await;

    // now send it back
    let gas_object = context
        .get_gas_objects_owned_by_address(zklogin_addr, None)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let txn = TestTransactionBuilder::new(zklogin_addr, gas_object, rgp)
        .transfer_sui(None, sender)
        .build();

    let (_, signed_txn, _) = sign_zklogin_tx(txn);

    context.execute_transaction_must_succeed(signed_txn).await;

    assert!(context
        .get_gas_objects_owned_by_address(zklogin_addr, None)
        .await
        .unwrap()
        .is_empty());
}

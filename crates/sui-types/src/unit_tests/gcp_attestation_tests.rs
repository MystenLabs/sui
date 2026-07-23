// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gcp_attestation::{
    GCP_ISSUER, GcpAttestationError, MAX_RSA_MODULUS_SIZE, MIN_RSA_MODULUS_SIZE, ParsedGcpJwt,
    RS256_ALG, is_gcp_attestation_call, rsa_exponent_ok, validate_rsa_public_key,
    verify_gcp_attestation,
};
use crate::{MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

// Pre-generated RSA-2048 test keys and pre-signed JWTs, all containing a `kid` header claim.
// Generated once via a throwaway Python `cryptography` script (not checked in).
// All tokens use NOW_MS = 1_700_000_000_000 as the reference time.
const NOW_MS: u64 = 1_700_000_000_000;
const TEST_KID: &str = "test-kid-001";

// Primary test key (key A) — base64url-encoded modulus and exponent.
const KEY_A_N: &str = "sZoorsILtRO6BjX7BTHj3CbUCsREm4VMikJCoixjwiYCPoAu7wcrE-EtuEluHPl6cTIR0osS8he6Y5BIYom_eYFpopkktpDt89enRl4E0dXOrCdwRjVpepnK3CL_eZSUnutuMtl172Opt7vnnotx4b4hzIuEI-ywhTM7iWbvSd98sc8u_STQjx4P1GT3hr4K-JehRxUae_g8nL-24dw8_H0G4u493FiHrxK4M9jHdIGtI9C19_uEZHydif2y70OwoIgs-GEhw2pjl0NZX_VZSXJ2J6mtHMxkYRI4PE18yjNM9MgmEnEptiGcUU1r6KO4fg5RSEVnLQCoDtFyd7uEvw";
const KEY_A_E: &str = "AQAB";

// Wrong key (key B) — different keypair for signature mismatch tests.
const KEY_B_N: &str = "pXZ4ZUpUzDQaYs8F9pnaSoNTGmafmLBUpoMVQdkZL2k4yhWE3Y4_pkvUR8URsQoIl1baX7abh_gjYkwzOkausJehttR0JpxzOraQ8pNIQ-8nMGT__T6N0IeW1pqyPoIqyiwPGKlyXC4RAinMpJ_TdpaNRSEYLMr7hjZ9WlYtSyi17tad7eFWDCKVXIjj24NxmZ-PGqkGiuMg2VlafJxjswlnQxipAwzkD3difajMMA0clI-hTxCv4YNlPI7ukDstdCwFN8bJ3CuW-2Vjivwu2IPGnNJoeQ6Ss_zKTv8dUjtf8wzj9XD20UBvyKeUnqc7l4793cESzudS4-GZMvIf7w";
const KEY_B_E: &str = "AQAB";

// Pre-signed JWT tokens (RS256, signed with key A), all with `kid`="test-kid-001" in the header.
// Valid: iss=GCP, exp=now+3600, iat=now-10
const VALID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDM2MDAsImlhdCI6MTY5OTk5OTk5MCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.igZOxBZWeu6YfHqQOoimOWfHSzl9Cr4tiyT2hQ-GMlLYXoQ_ln0u8p13VVFdFfrd2xLfpqNvhNOyq4DarhguSumyv9rmvBUiy_iQ5ZezpJtfMTOChmDVb-bgIDxF4221GR6DjfhuOTxwpVgz4vxnTqH5ABVAQ-mv_XC0MAO2xFhvBMtkDRdvEFPhETyWizS9M_We_M_zj5c35ug0no33_nS-u-bFCnArVcQNlVyHTtkrkhGE6d3-xHi5V3Fs3rYxqENOMdAx5EbCm2-g_jiXjGdahCb5QkGjtO_HYM2NJo4Z-fHZzYkCmSYJGeUqXKeSmGT2F2Y2dbdGdOU60pCp7Q";
// Expired: exp=now-1, iat=now-3600
const EXPIRED_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE2OTk5OTk5OTksImlhdCI6MTY5OTk5NjQwMCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.LNCAJMg3yq3uWkSdswWI4c0Sdr_BEUMTlPl2J0TiZlfvxyw1XeD9pdgAp4gawPeOSNjYtGhNVfV6PYNzohYUfzYEEporjkrUCMtPSfnD7qARxu03i7Ms9qlQekaanOlQxNSsarnN-CSJPVsQ1Z8bZQ1iRM60Nx1DiIIsK2QZ6HKYMFntTiLr-a2sQpvRuythicbCSuRoASW7Fam7J2fQQaGTUB1kY2fVRXvB53in33YZKKqGsbdOI4c_YR01OWtNoRXIHkdYzUgDLpSueIPjiE9pgBCCphp2a5HzXUfRii8BA7x60JKouNUXgslZbhSGO5O74TThy4FWf7F2HQ2n1A";
// Wrong issuer: iss=wrong.issuer.example.com
const WRONG_ISS_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL3dyb25nLmlzc3Vlci5leGFtcGxlLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDM2MDAsImlhdCI6MTY5OTk5OTk5MCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.DNH_b4oQZKO-fRZNDst4y4Ag3qjzLCKWxiNtzFq64mSQW6LP_QLd-G6viYcIMmfI6nZ06UaJAFV-x5PknKn9DrwljOS1Hpkj81C9ElFKd-GNGrCLiWvQpfIBOUQo61hEXChpYcPDPhsR5MqObYupr602QuNiblYb5UWZasMIa-GOWLzfd5zZZyaSzTF-WUQAr6MnvQJRi2uyseKUjy6BXEABpb1hHyUM9yX3SYewvu53wpkjXv5ORJu4jiaobadiYNUZWkgLaCHW9SX7T6NFuJROe9nO1JNgO8dmTghIRAOP2pndPk2iZWds8FICUNhzmTm4jMKH5nNvIoiqxggTHw";
// Future iat: iat=now+3600, exp=now+7200
const FUTURE_IAT_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDcyMDAsImlhdCI6MTcwMDAwMzYwMCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.jKux0m8RaBFYN3hrg00H9teAYxxPNkOkPi9cY9mVgI7oLATo19eXSVs5Yg7LDDe3PnErj3nWax6EwiWr5emonbWWCXbxRs0def7GYfYVYPLv4HURXDX51iWVBK2IFts9xW4svKWXs2Aq4QlvQYbAZ_VZfKJaTro0M0uAEbGXWuwSY0GuyZuypKAN28YF2lHruwv4sH1I_pCAaajp-qQcP_S3P84JjQ8ZCFCFvbC_gq5ZlqKnqwpbh9nhKG65tYdTC5XqMdsdN4vF5hPPpZPHh4bhWs8dnvt9ACzcjPJMwzot_ZBh7OqLi_V6NMDjCiNndY2Hl9ODwZ-2y_A_HksfAQ";
// Real GCP payload structure: exp=4000000000 (year 2096), iat=1700000000
const REAL_GCP_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6Imh0dHBzOi8vd3d3Lmdvb2dsZWFwaXMuY29tL2NvbXB1dGUvdjEvcHJvamVjdHMvazhzLWNsdXN0ZXJzLTM5NzUyMS96b25lcy91cy1jZW50cmFsMS1iL2luc3RhbmNlcy9jb25mc3BhY2UtaGVsbG8iLCJhdWQiOiJodHRwczovL215LXZlcmlmaWVyLmV4YW1wbGUiLCJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMCwibmJmIjoxNzAwMDAwMDAwLCJlYXRfbm9uY2UiOiI0MDdlNzcwMi1jZWM4LTQ0NmMtOGQ0Ni00NjY5ODcyMzU0M2MiLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJzd3ZlcnNpb24iOlsiMjUwMzAxIl0sImRiZ3N0YXQiOiJkaXNhYmxlZC1zaW5jZS1ib290Iiwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfcmVmZXJlbmNlIjoidXMtY2VudHJhbDEtZG9ja2VyLnBrZy5kZXYvazhzLWNsdXN0ZXJzLTM5NzUyMS9jb25mc3BhY2UtdGVzdC9oZWxsb193b3JsZEBzaGEyNTY6YWM5MWRkMzY4MTkzZWZhOTM4Nzc2ZjM1ZmY3MTVlMjlmOTA3YzJiNjY3MWJhZTY1ODlkNDRmNjBjOGQ4MmE1NCIsImltYWdlX2RpZ2VzdCI6InNoYTI1NjphYzkxZGQzNjgxOTNlZmE5Mzg3NzZmMzVmZjcxNWUyOWY5MDdjMmI2NjcxYmFlNjU4OWQ0NGY2MGM4ZDgyYTU0IiwicmVzdGFydF9wb2xpY3kiOiJOZXZlciJ9fX0.lyDVf3eXfFG8g63CjQ8T9D3F9P26aSdgLBIO91f_93oJG40pyVtuTqhD_8CUW3E91jz_sgeTykULD49ATs2bK1EoTxllL5PfKFqJQMsn8684yT8UROJO1WrQOBSmxixkNNn5GKFkn_5u_6egLW8ze35geuCX4_sR6GrRpmRetbtE2tpW9AdTJaz5GfeGISb9J48pkEoDzG3eEr2b4cO0t_g6j_0byxaTFy7N1nemp0dIluRdpIrioY6yJELRR0jwjG_fTPqOhv1iE-8E_U1rosnKgQyTbmVRORE83T6Rk9-4MwUCPZdmGIACjg3mZrwYGiyhbyHozKhy2Z772hDBFA";
// Well-formed structure (RS256, kid present) but an all-zero signature that cannot verify
// against any key.
const WELL_FORMED_BAD_SIG_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDM2MDAsImlhdCI6MTY5OTk5OTk5MCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
// Same claims as `VALID_TOKEN`, but the header has been tampered with post-signing
// (dropped the `typ` field) while `kid` still names a registered key.
const TAMPERED_HEADER_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDM2MDAsImlhdCI6MTY5OTk5OTk5MCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.igZOxBZWeu6YfHqQOoimOWfHSzl9Cr4tiyT2hQ-GMlLYXoQ_ln0u8p13VVFdFfrd2xLfpqNvhNOyq4DarhguSumyv9rmvBUiy_iQ5ZezpJtfMTOChmDVb-bgIDxF4221GR6DjfhuOTxwpVgz4vxnTqH5ABVAQ-mv_XC0MAO2xFhvBMtkDRdvEFPhETyWizS9M_We_M_zj5c35ug0no33_nS-u-bFCnArVcQNlVyHTtkrkhGE6d3-xHi5V3Fs3rYxqENOMdAx5EbCm2-g_jiXjGdahCb5QkGjtO_HYM2NJo4Z-fHZzYkCmSYJGeUqXKeSmGT2F2Y2dbdGdOU60pCp7Q";

// Structural fixtures with no valid signature; only used to exercise `ParsedGcpJwt::parse`.
const MISSING_KID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.e30.eA";
const EMPTY_KID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IiJ9.e30.eA";
const NONE_ALG_TOKEN: &str = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIiwia2lkIjoic29tZS1raWQifQ.e30.eA";

fn decode_key(n_b64: &str, e_b64: &str) -> (Vec<u8>, Vec<u8>) {
    (
        URL_SAFE_NO_PAD.decode(n_b64).expect("bad n"),
        URL_SAFE_NO_PAD.decode(e_b64).expect("bad e"),
    )
}

fn oversized_kid_token() -> String {
    let kid = "k".repeat(4097);
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": kid});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    format!("{header_b64}.e30.eA")
}

fn max_size_kid_token() -> String {
    let kid = "k".repeat(4096);
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": kid});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    format!("{header_b64}.e30.eA")
}

#[test]
fn test_valid_attestation() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let now_secs = NOW_MS / 1000;

    let doc =
        verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS).expect("should verify");
    assert_eq!(doc.iss, GCP_ISSUER.as_bytes());
    assert_eq!(doc.sub, b"test-subject");
    assert_eq!(doc.aud, b"test-audience");
    assert_eq!(doc.exp, now_secs + 3600);
    assert_eq!(doc.iat, now_secs - 10);
    assert_eq!(doc.eat_nonce, vec![b"nonce1".to_vec(), b"nonce2".to_vec()]);
    assert!(doc.secboot);
    assert_eq!(doc.hwmodel, b"GCP_AMD_SEV");
    assert_eq!(doc.swname, b"CONFIDENTIAL_SPACE");
    assert_eq!(doc.dbgstat, b"disabled-since-boot");
    assert_eq!(doc.swversion, vec![b"231031".to_vec()]);
    assert_eq!(doc.image_digest, b"sha256:abc123");
    assert_eq!(
        doc.image_reference,
        b"us-docker.pkg.dev/project/repo/image:latest"
    );
    assert_eq!(doc.restart_policy, b"Never");
}

#[test]
fn test_expired_token() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let err = verify_gcp_attestation(EXPIRED_TOKEN.as_bytes(), &n, &e, NOW_MS).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("token has expired".to_string())
    );
}

#[test]
fn test_invalid_issuer() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let err = verify_gcp_attestation(WRONG_ISS_TOKEN.as_bytes(), &n, &e, NOW_MS).unwrap_err();
    match err {
        GcpAttestationError::VerifyError(msg) => assert!(msg.contains("invalid issuer")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn test_wrong_key() {
    let (n, e) = decode_key(KEY_B_N, KEY_B_E);
    let err = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("signature verification failed".to_string())
    );
}

#[test]
fn test_tampered_header_fails_verify_for_registered_key() {
    // The kid in the tampered header still names a registered key (key A), but the header
    // bytes differ from what was signed, so signature verification -- not kid lookup -- must
    // be what fails. This is what the native layer maps to VERIFY_ERROR.
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let parsed = ParsedGcpJwt::parse(TAMPERED_HEADER_TOKEN.as_bytes()).expect("should parse");
    assert_eq!(parsed.kid(), TEST_KID);
    let err = parsed.verify(&n, &e, NOW_MS).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("signature verification failed".to_string())
    );
}

#[test]
fn test_tampered_payload() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let parts: Vec<&str> = VALID_TOKEN.splitn(3, '.').collect();
    let tampered_payload = URL_SAFE_NO_PAD.encode(
        b"{\"iss\":\"https://confidentialcomputing.googleapis.com\",\"exp\":9999999999,\"iat\":0}",
    );
    let tampered = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);
    let err = verify_gcp_attestation(tampered.as_bytes(), &n, &e, NOW_MS).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("signature verification failed".to_string())
    );
}

#[test]
fn test_reject_none_algorithm() {
    let err = ParsedGcpJwt::parse(NONE_ALG_TOKEN.as_bytes()).unwrap_err();
    match err {
        GcpAttestationError::VerifyError(msg) => assert!(msg.contains("unsupported algorithm")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn test_oversized_token() {
    let oversized_token = vec![b'a'; 16 * 1024 + 1];
    let err = ParsedGcpJwt::parse(&oversized_token).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("JWT token too large".to_string())
    );
}

#[test]
fn test_oversized_modulus() {
    let oversized_n = vec![0u8; MAX_RSA_MODULUS_SIZE + 1];
    let e = vec![0x01, 0x00, 0x01];
    let err = validate_rsa_public_key(&oversized_n, &e).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA modulus size out of bounds".to_string())
    );
}

#[test]
fn test_undersized_modulus() {
    let undersized_n = vec![0u8; MIN_RSA_MODULUS_SIZE - 1];
    let e = vec![0x01, 0x00, 0x01];
    let err = validate_rsa_public_key(&undersized_n, &e).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA modulus size out of bounds".to_string())
    );
}

#[test]
fn test_invalid_rsa_key_fails_verify_before_signature_check() {
    // Well-formed token (valid structure, header, and kid) but the caller-supplied key is
    // invalid; validate_rsa_public_key must reject before any signature work occurs.
    let n = vec![0x00];
    let e = vec![0x00];
    let parsed = ParsedGcpJwt::parse(WELL_FORMED_BAD_SIG_TOKEN.as_bytes()).expect("should parse");
    let err = parsed.verify(&n, &e, NOW_MS).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA modulus size out of bounds".to_string())
    );
}

#[test]
fn test_future_iat() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let err = verify_gcp_attestation(FUTURE_IAT_TOKEN.as_bytes(), &n, &e, NOW_MS).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("token issued in the future".to_string())
    );
}

#[test]
fn test_real_gcp_payload_claims() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let doc =
        verify_gcp_attestation(REAL_GCP_TOKEN.as_bytes(), &n, &e, NOW_MS).expect("should verify");

    assert_eq!(doc.iss, b"https://confidentialcomputing.googleapis.com");
    assert_eq!(
        doc.sub,
        b"https://www.googleapis.com/compute/v1/projects/k8s-clusters-397521/zones/us-central1-b/instances/confspace-hello"
    );
    assert_eq!(doc.aud, b"https://my-verifier.example");
    assert_eq!(doc.exp, 4_000_000_000);
    assert_eq!(doc.iat, 1_700_000_000);
    assert_eq!(
        doc.eat_nonce,
        vec![b"407e7702-cec8-446c-8d46-46698723543c".to_vec()]
    );
    assert!(doc.secboot);
    assert_eq!(doc.hwmodel, b"GCP_AMD_SEV");
    assert_eq!(doc.swname, b"CONFIDENTIAL_SPACE");
    assert_eq!(doc.swversion, vec![b"250301".to_vec()]);
    assert_eq!(doc.dbgstat, b"disabled-since-boot");
    assert_eq!(
        doc.image_digest,
        b"sha256:ac91dd368193efa938776f35ff715e29f907c2b6671bae6589d44f60c8d82a54"
    );
    assert_eq!(
        doc.image_reference,
        b"us-central1-docker.pkg.dev/k8s-clusters-397521/confspace-test/hello_world@sha256:ac91dd368193efa938776f35ff715e29f907c2b6671bae6589d44f60c8d82a54"
    );
    assert_eq!(doc.restart_policy, b"Never");
}

#[test]
fn test_parse_extracts_kid_without_reparsing_header() {
    let parsed = ParsedGcpJwt::parse(VALID_TOKEN.as_bytes()).expect("should parse");
    assert_eq!(parsed.kid(), TEST_KID);
    // Calling verify() must not need to touch the header again; kid() remains stable.
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    parsed.verify(&n, &e, NOW_MS).expect("should verify");
    assert_eq!(parsed.kid(), TEST_KID);
}

#[test]
fn test_parse_rejects_missing_kid() {
    let err = ParsedGcpJwt::parse(MISSING_KID_TOKEN.as_bytes()).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("missing 'kid' in header".to_string())
    );
}

#[test]
fn test_parse_rejects_empty_kid() {
    let err = ParsedGcpJwt::parse(EMPTY_KID_TOKEN.as_bytes()).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("empty 'kid' in header".to_string())
    );
}

#[test]
fn test_parse_rejects_oversized_kid() {
    let token = oversized_kid_token();
    let err = ParsedGcpJwt::parse(token.as_bytes()).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("'kid' exceeds maximum size".to_string())
    );
}

#[test]
fn test_parse_accepts_max_size_kid() {
    let token = max_size_kid_token();
    let parsed = ParsedGcpJwt::parse(token.as_bytes()).expect("4096-byte kid should be accepted");
    assert_eq!(parsed.kid().len(), 4096);
}

#[test]
fn test_reject_weak_exponent_padded_e3() {
    let (n, _) = decode_key(KEY_A_N, KEY_A_E);
    // Numerically e=3, padded to 3 bytes — must reject (not just length check).
    let e = vec![0x00, 0x00, 0x03];
    let err = validate_rsa_public_key(&n, &e).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA exponent out of bounds".to_string())
    );
}

#[test]
fn test_reject_weak_exponent_e3() {
    let (n, _) = decode_key(KEY_A_N, KEY_A_E);
    let e = vec![0x03];
    let err = validate_rsa_public_key(&n, &e).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA exponent out of bounds".to_string())
    );
}

#[test]
fn test_reject_even_exponent() {
    let (n, _) = decode_key(KEY_A_N, KEY_A_E);
    // 65538 is > 65537 numerically but even — must still be rejected.
    let e = vec![0x01, 0x00, 0x02];
    let err = validate_rsa_public_key(&n, &e).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA exponent out of bounds".to_string())
    );
}

#[test]
fn test_accept_min_exponent_65537() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    // KEY_A_E is AQAB == [1,0,1] == 65537; verify still succeeds.
    validate_rsa_public_key(&n, &e).expect("should accept minimum valid exponent");
    verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS).expect("should verify");
}

#[test]
fn test_rsa_exponent_ok_matches_validate_rsa_public_key() {
    // rsa_exponent_ok remains available as a standalone building block, reused by
    // validate_rsa_public_key.
    assert!(rsa_exponent_ok(&[0x01, 0x00, 0x01]));
    assert!(!rsa_exponent_ok(&[0x03]));
}

#[test]
fn test_is_gcp_attestation_call_matches_only_framework_entrypoint() {
    assert!(is_gcp_attestation_call(
        SUI_FRAMEWORK_ADDRESS,
        "gcp_attestation",
        "verify_gcp_attestation",
    ));
    assert!(!is_gcp_attestation_call(
        MOVE_STDLIB_ADDRESS,
        "gcp_attestation",
        "verify_gcp_attestation",
    ));
    assert!(!is_gcp_attestation_call(
        SUI_FRAMEWORK_ADDRESS,
        "nitro_attestation",
        "verify_gcp_attestation",
    ));
    assert!(!is_gcp_attestation_call(
        SUI_FRAMEWORK_ADDRESS,
        "gcp_attestation",
        "load_nitro_attestation",
    ));
}

#[test]
fn test_gcp_issuer_and_alg_constants() {
    assert_eq!(GCP_ISSUER, "https://confidentialcomputing.googleapis.com");
    assert_eq!(RS256_ALG, "RS256");
}

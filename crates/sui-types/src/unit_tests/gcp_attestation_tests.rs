// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gcp_attestation::{
    GcpAttestationError, extract_kid_from_jwt, is_gcp_attestation_call, verify_gcp_attestation,
};
use crate::{MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

const EXPECTED_ISSUER: &str = "https://confidentialcomputing.googleapis.com";

// Pre-generated RSA-2048 test keys and pre-signed JWTs.
// Generated once via crates/sui-types/tests/gen_gcp_test_vectors.rs (now deleted).
// All tokens use NOW_MS = 1_700_000_000_000 as the reference time.
const NOW_MS: u64 = 1_700_000_000_000;

// Primary test key (key A) — base64url-encoded modulus and exponent
const KEY_A_N: &str = "5Q_dKcwT2Bn7jh4qXmNjzsIHtgXRMDAYcOedHYJIZG-qglZg_ZMdmUwp4tF8lL9kXUZ09OvkwCdrH28rm87hA2UookBxHCQL0VIpJnykusCy2pqFb198TQ4xp4GvEgCY823nex6PpV_q-R2efGqMAg6I3VeFb9Fs0-dpDZ_KNZYse3c3y3RromaBK8nXg4dpHEta7i1Em_jaCzXOqwpr0SWJq7J0L6mKCh9jzsETXfzCvQYPG0LC0eZ2cpBViWCZ5iwPN7Wh994my0WWZ5p0zhgNCQsso4e0VlBWii6rjVqZfX-EHMuz5pvzXwlarWy9_L_65SEdM7kgGhsSyg-Hlw";
const KEY_A_E: &str = "AQAB";

// Wrong key (key B) — different keypair for signature mismatch tests
const KEY_B_N: &str = "0kmExZ-vhOpqUO8Ff0H8QAAihAa7aVRT-Qugg7D91hadHyzxktnCfoNxOc2orboUfLLOf6OIuQqG7mGFjTz4dKCrbmAHutwJaJvjlPavcDWPPv2i4LT6-m-Qqdploml1Osi1jVEsDjWzI3DWCpe-_umgRXGOhzfUq7klr0-Wu8shCoJVsxXxSntcZBHpcKOtU6A0yXgPi7AvQGUDhcxBKeAGlELt7RSde6dc0SA-0omZok174o4n8vnw9rZpNjuGP_V9qXiDGLg2OsciK8JmMmA3CDU-i_H0BoSmg8xjS4mMF-2i70DXdv_NAlJFeTYXZK1_58pXUWS47uY5OhBAqw";
const KEY_B_E: &str = "AQAB";

// Pre-signed JWT tokens (RS256, signed with key A)
// Valid: iss=GCP, exp=now+3600, iat=now-10
const VALID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDM2MDAsImlhdCI6MTY5OTk5OTk5MCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.aGiTXA-4LCk800gwBj6TOKQKyp-MNnFD5uskbre4kvMCiFA0LZWYpinDv2BLQGQ4g6kLUUkVlQr2JyT05ycHaZ8s6KefF1G_N0TcHOK6WI3McR3Ym8aLTXFHr0sUtEbgZ15P8XOsaKItDMC33Pt6ZGGnxEkFG0gDvmoRbEtvmx9YwnJrycwqWngaUD-lYpC07xk9dhhws5n6h-e_10K-kSxfYi21rlNeaw9V9rgYirGPTTdy3I_8m--4AyTmro7lqXv-5Ep2D_A1USMpDtmcZ2fg7WWXu1NxraW7zgmO57r86AOAUIqRBjwF-H5fhU123rc0SiUFQEEAcIUXvAEE_Q";
// Expired: exp=now-1, iat=now-3600
const EXPIRED_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE2OTk5OTk5OTksImlhdCI6MTY5OTk5NjQwMCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.XjHRrIT_aWAjnRxL2cB_U2J-nMTxywwqBIN3SC5IDiGt2ea5gK1Msc-KYZFirmzuuKgMHENbehXWFSsOfyXPBexqlVQQlzSNdwUvw8FYeucyR9qWHdYYjaQFJWAUcHEbXbSoVoQ3_mV5XW4d2HbbC3d3NLNtuYSpYqTB8kQNLLqBsLsForYx5QE-0McQBDd0Sqa3d9AWYkYB9KLPvY9EB3mjlmdyB_IYl7MWi_hA4DIy1rK5lII4LP6e_f7laDbCuLowQaJ9xX2amlf_CnPktprPUe_MTHw350ZppeHJ5EyUHMM_I2ofNaAuc_rKmUOccmWx0CHU0dnv4qEyszaxvA";
// Wrong issuer: iss=wrong.issuer.example.com
const WRONG_ISS_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3dyb25nLmlzc3Vlci5leGFtcGxlLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDM2MDAsImlhdCI6MTY5OTk5OTk5MCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.BLUZGV_h1jZibacPiJ2_kgiS6MEyMacH3v_nVvtW3F3FKEGyYvm7t9hjMh35rkNv3u5jb7if7GRDTSsvEe09CXMTwxmZSLQh6Q57ppaMWDtDCRzqMivcvC8kS1fGA14CAcWUm2F_i2XrVpMnn2wZY33UpXa4nU-7Hk3jQnfd2D0LG-Blynmx3fuBVQZG4lqZKwmpF1-Y9hJJhleF_RNW6zCe_P2-1d5W-uJZOlfZ2LLi6TfFi7Kr2I2Ft2l_KtM28LV4kzmvKfEn_yKZco8LD1UpJPmemYxxIDYuRXYfmSw2GLEsLm0h_NCJD5uQso2P0eYF9OEb4_-GDhilJusfVw";
// Future iat: iat=now+3600, exp=now+7200
const FUTURE_IAT_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjE3MDAwMDcyMDAsImlhdCI6MTcwMDAwMzYwMCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.FNlNYhkpbYi6m0ggRL48q9t3UK1rZL3WDkv-SawfeUiuVh1MIIH972ezp-yKDocp8jnqFNxo150XX76ngG1SgslcqFqj0Xm-ihMrLyQ-78AvzWpElNqfFXsSFeDAT2dC-8GraTF7RIPlM4ewFZ1_XFNSpUQVw_QkTGqYBNO6pCuzQRCV9eF6cROI598j7ZMTK87Is-qqRWk0Ndw_UENpJdj8GRSl-kjOpeU4l15b9e9LTrZKX252RgC2jziHhjsMSACA_WHlCC_pn6wvba4J7_U8KAls7fdFcW18PwBM4WioClRsiBs2tgAiddYtSU0lagATCT6NamYKqZIn7oe-gQ";
// Real GCP payload structure: exp=4000000000 (year 2096), iat=1700000000
const REAL_GCP_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6Imh0dHBzOi8vd3d3Lmdvb2dsZWFwaXMuY29tL2NvbXB1dGUvdjEvcHJvamVjdHMvazhzLWNsdXN0ZXJzLTM5NzUyMS96b25lcy91cy1jZW50cmFsMS1iL2luc3RhbmNlcy9jb25mc3BhY2UtaGVsbG8iLCJhdWQiOiJodHRwczovL215LXZlcmlmaWVyLmV4YW1wbGUiLCJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMCwibmJmIjoxNzAwMDAwMDAwLCJlYXRfbm9uY2UiOiI0MDdlNzcwMi1jZWM4LTQ0NmMtOGQ0Ni00NjY5ODcyMzU0M2MiLCJlYXRfcHJvZmlsZSI6Imh0dHBzOi8vY2xvdWQuZ29vZ2xlLmNvbS9jb25maWRlbnRpYWwtY29tcHV0aW5nL2NvbmZpZGVudGlhbC1zcGFjZS9kb2NzL3JlZmVyZW5jZS90b2tlbi1jbGFpbXMiLCJzZWNib290Ijp0cnVlLCJvZW1pZCI6MTExMjksImh3bW9kZWwiOiJHQ1BfQU1EX1NFViIsInN3bmFtZSI6IkNPTkZJREVOVElBTF9TUEFDRSIsInN3dmVyc2lvbiI6WyIyNTAzMDEiXSwiZGJnc3RhdCI6ImRpc2FibGVkLXNpbmNlLWJvb3QiLCJzdWJtb2RzIjp7ImNvbmZpZGVudGlhbF9zcGFjZSI6eyJzdXBwb3J0X2F0dHJpYnV0ZXMiOlsiTEFURVNUIiwiU1RBQkxFIiwiVVNBQkxFIl0sIm1vbml0b3JpbmdfZW5hYmxlZCI6eyJtZW1vcnkiOmZhbHNlfX0sImNvbnRhaW5lciI6eyJpbWFnZV9yZWZlcmVuY2UiOiJ1cy1jZW50cmFsMS1kb2NrZXIucGtnLmRldi9rOHMtY2x1c3RlcnMtMzk3NTIxL2NvbmZzcGFjZS10ZXN0L2hlbGxvX3dvcmxkQHNoYTI1NjphYzkxZGQzNjgxOTNlZmE5Mzg3NzZmMzVmZjcxNWUyOWY5MDdjMmI2NjcxYmFlNjU4OWQ0NGY2MGM4ZDgyYTU0IiwiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFjOTFkZDM2ODE5M2VmYTkzODc3NmYzNWZmNzE1ZTI5ZjkwN2MyYjY2NzFiYWU2NTg5ZDQ0ZjYwYzhkODJhNTQiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIiwiaW1hZ2VfaWQiOiJzaGEyNTY6ODJhOTZiOTM2YjIyMzY2NTI2YTRiMWE5NjBkYTNiMjYxMzVlNzc3N2Q1Yzg2ZWNjNGY0ODc0MmVjMGE2NTU1NyIsImVudiI6eyJIT1NUTkFNRSI6ImNvbmZzcGFjZS1oZWxsbyIsIlBBVEgiOiIvdXNyL2xvY2FsL3NiaW46L3Vzci9sb2NhbC9iaW46L3Vzci9zYmluOi91c3IvYmluOi9zYmluOi9iaW4iLCJTU0xfQ0VSVF9GSUxFIjoiL2V0Yy9zc2wvY2VydHMvY2EtY2VydGlmaWNhdGVzLmNydCJ9LCJhcmdzIjpbIi9oZWxsb193b3JsZCJdfSwiZ2NlIjp7InpvbmUiOiJ1cy1jZW50cmFsMS1iIiwicHJvamVjdF9pZCI6Ims4cy1jbHVzdGVycy0zOTc1MjEiLCJwcm9qZWN0X251bWJlciI6IjY2MDg5MjIyNzQwNyIsImluc3RhbmNlX25hbWUiOiJjb25mc3BhY2UtaGVsbG8iLCJpbnN0YW5jZV9pZCI6IjU4NzczNjMzMzAwNzU5ODY1MiJ9fSwiZ29vZ2xlX3NlcnZpY2VfYWNjb3VudHMiOlsiY29uZnNwYWNlLXJ1bm5lckBrOHMtY2x1c3RlcnMtMzk3NTIxLmlhbS5nc2VydmljZWFjY291bnQuY29tIl19.0Pk0nTcM35tGU-fb6jCv-HEZfKXjklI4MqYBUpJqCy98wqkzZ9QR4EvB-fIEZC1zfgkYTvqgtOxco7Q-56oa8rzLnIG_6uC7HwfrkXL9ASeSS6kk2ygyapN-zJrlfsdHOoWM7ILNH9UtBVk0h-IPHgra80qMQO41_X75rBHY30g39dyPUMmanN-rh0M-nFPDU-WyAGehfbbjPGWZS0oodu1Tk4v1C6SmeNclYEW2-DKKsxSZyzZgBq46BUws7ZOrf84eW8qfeEGHOA6b29qQZivUdfkwR1txH4ZwI_oPO8c5-p6DI-1gfY8C5sh9-BRQelhIo9WMUMRbbKE4lbcAeQ";

fn decode_key(n_b64: &str, e_b64: &str) -> (Vec<u8>, Vec<u8>) {
    (
        URL_SAFE_NO_PAD.decode(n_b64).expect("bad n"),
        URL_SAFE_NO_PAD.decode(e_b64).expect("bad e"),
    )
}

#[test]
fn test_valid_attestation() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let now_secs = NOW_MS / 1000;

    let doc = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, None)
        .expect("should verify");
    assert_eq!(doc.iss, EXPECTED_ISSUER.as_bytes());
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
    let err = verify_gcp_attestation(EXPIRED_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("token has expired".to_string())
    );
}

#[test]
fn test_invalid_issuer() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let err = verify_gcp_attestation(WRONG_ISS_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    match err {
        GcpAttestationError::VerifyError(msg) => assert!(msg.contains("invalid issuer")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn test_wrong_key() {
    let (n, e) = decode_key(KEY_B_N, KEY_B_E);
    let err = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
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
    let err = verify_gcp_attestation(tampered.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("signature verification failed".to_string())
    );
}

#[test]
fn test_reject_none_algorithm() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let header = serde_json::json!({"alg": "none", "typ": "JWT"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(
        b"{\"iss\":\"https://confidentialcomputing.googleapis.com\",\"exp\":9999999999,\"iat\":0}",
    );
    let token = format!("{}.{}.", header_b64, payload_b64);
    let err = verify_gcp_attestation(token.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    match err {
        GcpAttestationError::VerifyError(msg) => assert!(msg.contains("unsupported algorithm")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn test_oversized_token() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let oversized_token = vec![b'a'; 16 * 1024 + 1];
    let err = verify_gcp_attestation(&oversized_token, &n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("JWT token too large".to_string())
    );
}

#[test]
fn test_oversized_modulus() {
    let oversized_n = vec![0u8; 513];
    let e = vec![0x01, 0x00, 0x01];
    let token = b"a.b.c";
    let err = verify_gcp_attestation(token, &oversized_n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA modulus size out of bounds".to_string())
    );
}

#[test]
fn test_invalid_rsa_key() {
    let n = vec![0x00];
    let e = vec![0x00];
    let header_b64 = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"RS256\"}");
    let payload_b64 = URL_SAFE_NO_PAD.encode(
        b"{\"iss\":\"https://confidentialcomputing.googleapis.com\",\"exp\":9999999999,\"iat\":0}",
    );
    let sig_b64 = URL_SAFE_NO_PAD.encode(b"fakesig");
    let token = format!("{}.{}.{}", header_b64, payload_b64, sig_b64);
    let err = verify_gcp_attestation(token.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    assert!(matches!(
        err,
        GcpAttestationError::VerifyError(_) | GcpAttestationError::ParseError(_)
    ));
}

#[test]
fn test_future_iat() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let err =
        verify_gcp_attestation(FUTURE_IAT_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("token issued in the future".to_string())
    );
}

#[test]
fn test_real_gcp_payload_claims() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let doc = verify_gcp_attestation(REAL_GCP_TOKEN.as_bytes(), &n, &e, NOW_MS, None)
        .expect("should verify");

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
fn test_extract_kid_from_jwt() {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": "key-id-abc123"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let token = format!("{}.fakepayload.fakesig", header_b64);
    let kid = extract_kid_from_jwt(token.as_bytes()).expect("should extract kid");
    assert_eq!(kid, "key-id-abc123");
}

#[test]
fn test_extract_kid_missing() {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let token = format!("{}.fakepayload.fakesig", header_b64);
    let err = extract_kid_from_jwt(token.as_bytes()).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("missing 'kid' in header".to_string())
    );
}

#[test]
fn test_reject_weak_exponent_padded_e3() {
    let (n, _) = decode_key(KEY_A_N, KEY_A_E);
    // Numerically e=3, padded to 3 bytes — must reject (not just length check).
    let e = vec![0x00, 0x00, 0x03];
    let err = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA exponent out of bounds".to_string())
    );
}

#[test]
fn test_reject_weak_exponent_e3() {
    let (n, _) = decode_key(KEY_A_N, KEY_A_E);
    let e = vec![0x03];
    let err = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
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
    let err = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, None).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA exponent out of bounds".to_string())
    );
}

#[test]
fn test_kid_mismatch() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": "actual-kid"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(
        b"{\"iss\":\"https://confidentialcomputing.googleapis.com\",\"exp\":9999999999,\"iat\":0}",
    );
    let token = format!("{}.{}.fakesig", header_b64, payload_b64);
    let err =
        verify_gcp_attestation(token.as_bytes(), &n, &e, NOW_MS, Some("other-kid")).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("kid mismatch".to_string())
    );
}

#[test]
fn test_kid_missing_when_expected() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    // VALID_TOKEN has no kid in the header.
    let err = verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, Some("any-kid"))
        .unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("missing 'kid' in header".to_string())
    );
}

#[test]
fn test_accept_min_exponent_65537() {
    let (n, e) = decode_key(KEY_A_N, KEY_A_E);
    // KEY_A_E is AQAB == [1,0,1] == 65537; verify still succeeds with None kid.
    verify_gcp_attestation(VALID_TOKEN.as_bytes(), &n, &e, NOW_MS, None).expect("should verify");
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

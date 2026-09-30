// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Client for the faucet's proof-of-work API, `GET /v3/challenge` and `POST /v3/gas`.
//!
//! The faucet pays out only after the client spends a server-chosen number of expected Argon2d
//! attempts on a preimage that binds recent public chain state, the faucet, and the recipient.
//! This module implements proof-of-work version 1 (`sui-faucet-pow/1`), which fixes the preimage
//! layout and every hashing parameter below.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use argon2::Algorithm;
use argon2::Argon2;
use argon2::Block;
use argon2::Params;
use argon2::Version;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use fastcrypto::hash::HashFunction;
use fastcrypto::hash::Sha256;
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use sui_types::base_types::SuiAddress;
use sui_types::gas_coin::MIST_PER_SUI;

use crate::client_commands::USER_AGENT;

const POW_VERSION: u64 = 1;
const POW_DOMAIN: &str = "sui-faucet-pow/1";
const POW_SALT: &str = "sui-faucet-pow-1";
const ARGON2_ALGORITHM: &str = "argon2d";
const ARGON2_VERSION: u32 = 0x13;
const ARGON2_MEMORY_KIB: u32 = 8192;
const ARGON2_ITERATIONS: u32 = 1;
const ARGON2_PARALLELISM: u32 = 1;
const HASH_LEN: usize = 32;

/// The domain of `threshold = floor(2^64 / difficulty)`. At 1 the threshold is 2^64, which does
/// not fit in a u64, and past 2^48 it stops distinguishing difficulties.
const MIN_DIFFICULTY: u64 = 2;
const MAX_DIFFICULTY: u64 = 1 << 48;

/// How long one proof may grind before this client gives up. The difficulty range bounds what the
/// threshold can represent, not what this client can afford: difficulty 2^48 means 2^48 expected
/// 8 MiB hashes. This budget is what limits the work a faucet can demand.
const MAX_SOLVE_TIME: Duration = Duration::from_secs(5 * 60);

/// The total covers reading the body, so a faucet cannot hold a request open by trickling bytes.
/// It is generous because a `POST /v3/gas` that times out leaves the payout's outcome unknown.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Challenges, payouts, and rejections are a few hundred bytes of JSON. The cap leaves room for a
/// proxy's HTML error page.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// How many proofs one request may grind. A proof is rejected as stale when the chain moves past
/// its freshness window before submission, and the rejection carries a fresh challenge to retry.
const MAX_PROOF_ATTEMPTS: usize = 3;

/// Where `sui client faucet` sends its request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FaucetEndpoint {
    /// A faucet serving the proof-of-work API, identified by its base URL. The path always ends
    /// in `/` so that API paths can be joined onto it.
    ProofOfWork(Url),
    /// A `/v1/gas` or `/v2/gas` endpoint that pays without proof of work, such as the local faucet
    /// that `sui start --with-faucet` runs.
    WithoutProofOfWork(String),
}

/// A payout the faucet executed, as reported by `POST /v3/gas`.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Payout {
    pub(crate) digest: String,
    pub(crate) recipient: String,
    amount_mist: String,
}

/// The response of `GET /v3/challenge`, and of the `challenge` field that some rejections carry,
/// before any of it is checked. u64 values arrive as decimal strings so that JSON cannot round
/// them.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RawChallenge {
    version: u64,
    domain: String,
    salt: String,
    algorithm: String,
    argon2_version: u32,
    memory_size: u32,
    iterations: u32,
    parallelism: u32,
    hash_length: usize,
    chain_id: String,
    checkpoint_seq: String,
    checkpoint_digest: String,
    random_bytes: String,
    faucet_address: String,
    recipient: String,
    difficulty: String,
}

/// A challenge checked against the protocol version this client implements, reduced to what
/// grinding and submission need.
#[derive(Debug)]
struct Challenge {
    /// The preimage up to and including the newline before the nonce, which is its last line.
    preimage_prefix: String,
    checkpoint_seq: u64,
    recipient: SuiAddress,
    difficulty: u64,
}

struct Proof {
    nonce: u64,
    hash: [u8; HASH_LEN],
}

/// A failed `POST /v3/gas`, carrying the faucet's machine-readable `code` when it sent one.
#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Rejection {
    #[serde(skip)]
    status: reqwest::StatusCode,
    error: Option<String>,
    code: Option<String>,
    /// The payout transaction, when the faucet may already have submitted one for this proof.
    digest: Option<String>,
    challenge: Option<serde_json::Value>,
    preimage_sha256: Option<String>,
}

/// Sets its flag when dropped, telling a `grind` on a blocking thread to stop.
struct CancelOnDrop(Arc<AtomicBool>);

impl FaucetEndpoint {
    /// Accepts a faucet's base URL, its `/v3/gas` URL, or a `/v1/gas` or `/v2/gas` URL.
    pub(crate) fn parse(url: &str) -> anyhow::Result<Self> {
        let mut parsed = Url::parse(url).with_context(|| format!("Invalid faucet URL: {url}"))?;
        let path = parsed.path().trim_end_matches('/');
        if path.ends_with("/v1/gas") || path.ends_with("/v2/gas") {
            return Ok(Self::WithoutProofOfWork(url.to_owned()));
        }
        let base = format!("{}/", path.strip_suffix("/v3/gas").unwrap_or(path));
        parsed.set_path(&base);
        Ok(Self::ProofOfWork(parsed))
    }
}

impl Payout {
    /// The paid amount in SUI, falling back to the faucet's raw MIST string if it does not parse.
    pub(crate) fn amount(&self) -> String {
        match self.amount_mist.parse::<u64>() {
            Ok(mist) => format_sui(mist),
            Err(_) => format!("{} MIST", self.amount_mist),
        }
    }
}

impl Challenge {
    fn parse(raw: RawChallenge, recipient: SuiAddress) -> anyhow::Result<Self> {
        // The spec requires rejecting any other parameters: they would either produce proofs the
        // faucet rejects, or let the faucet choose how much memory and time this client spends.
        ensure!(
            raw.version == POW_VERSION
                && raw.domain == POW_DOMAIN
                && raw.salt == POW_SALT
                && raw.algorithm == ARGON2_ALGORITHM
                && raw.argon2_version == ARGON2_VERSION
                && raw.memory_size == ARGON2_MEMORY_KIB
                && raw.iterations == ARGON2_ITERATIONS
                && raw.parallelism == ARGON2_PARALLELISM
                && raw.hash_length == HASH_LEN,
            "The faucet asks for proof-of-work version {} ({} with m={}, t={}, p={}), but this CLI \
             implements {POW_DOMAIN}. Updating the Sui CLI may fix this.",
            raw.version,
            raw.algorithm,
            raw.memory_size,
            raw.iterations,
            raw.parallelism,
        );
        ensure!(
            raw.recipient == recipient.to_string(),
            "The faucet returned a challenge for recipient {}, not {recipient}",
            raw.recipient,
        );

        let checkpoint_seq: u64 = raw
            .checkpoint_seq
            .parse()
            .with_context(|| format!("Invalid checkpointSeq {:?}", raw.checkpoint_seq))?;
        let difficulty: u64 = raw
            .difficulty
            .parse()
            .with_context(|| format!("Invalid difficulty {:?}", raw.difficulty))?;
        ensure!(
            (MIN_DIFFICULTY..=MAX_DIFFICULTY).contains(&difficulty),
            "The faucet asks for difficulty {difficulty}, outside the supported range \
             {MIN_DIFFICULTY} to {MAX_DIFFICULTY}",
        );
        let faucet_address: SuiAddress = raw
            .faucet_address
            .parse()
            .with_context(|| format!("Invalid faucetAddress {:?}", raw.faucet_address))?;

        // Addresses and integers are re-rendered in their canonical forms, while the digest and
        // randomness are hashed exactly as the faucet encoded them.
        let preimage_prefix = format!(
            "{POW_DOMAIN}\n{}\n{checkpoint_seq}\n{}\n{}\n{faucet_address}\n{recipient}\n",
            raw.chain_id, raw.checkpoint_digest, raw.random_bytes,
        );

        Ok(Self {
            preimage_prefix,
            checkpoint_seq,
            recipient,
            difficulty,
        })
    }

    fn preimage(&self, nonce: u64) -> String {
        format!("{}{nonce}", self.preimage_prefix)
    }

    async fn solve(&self) -> anyhow::Result<Proof> {
        eprintln!(
            "Solving the faucet's proof of work (difficulty {}) on 1 thread...",
            self.difficulty
        );

        let started = Instant::now();
        let preimage_prefix = self.preimage_prefix.clone();
        let threshold = threshold(self.difficulty);
        let start_nonce = rand::random();
        // Neither a timeout nor dropping the join handle stops a running blocking task, so grind
        // polls a flag that the guard sets however this function ends, including when a caller
        // drops the future.
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel = CancelOnDrop(cancelled.clone());
        let grinding = tokio::task::spawn_blocking(move || {
            grind(&preimage_prefix, threshold, start_nonce, &cancelled)
        });
        let Ok(joined) = tokio::time::timeout(MAX_SOLVE_TIME, grinding).await else {
            bail!(
                "Gave up on the faucet's proof of work (difficulty {}) after {}s. The faucet asks \
                 for more work than this CLI spends on one proof.",
                self.difficulty,
                MAX_SOLVE_TIME.as_secs(),
            );
        };
        let (proof, attempts) = joined?.context("The proof-of-work search was cancelled")?;
        eprintln!(
            "Solved after {attempts} attempts in {:.3}s.",
            started.elapsed().as_secs_f64()
        );
        Ok(proof)
    }
}

impl Rejection {
    fn parse(status: reqwest::StatusCode, body: &str) -> Self {
        let mut rejection = serde_json::from_str(body).unwrap_or_else(|_| Rejection {
            error: (!body.trim().is_empty()).then(|| body.trim().to_owned()),
            ..Default::default()
        });
        rejection.status = status;
        rejection
    }

    fn message(&self) -> String {
        let reason = self
            .error
            .clone()
            .unwrap_or_else(|| self.status.to_string());
        match &self.code {
            Some(code) => format!("{reason} ({code})"),
            None => reason,
        }
    }

    fn into_error(self, challenge: &Challenge, proof: &Proof) -> anyhow::Error {
        let message = self.message();
        if let Some(digest) = &self.digest {
            return anyhow!(
                "Faucet request was unsuccessful: {message}. The faucet may already have paid out \
                 in transaction {digest}; check that transaction before requesting again."
            );
        }
        match self.code.as_deref() {
            Some("invalid_proof") => {
                let client_sha256 =
                    Hex::encode(Sha256::digest(challenge.preimage(proof.nonce).as_bytes()));
                let faucet_sha256 = self.preimage_sha256.as_deref().unwrap_or("unknown");
                anyhow!(
                    "Faucet rejected the proof: {message}. SHA-256 of the faucet's preimage is \
                     {faucet_sha256}, and of this CLI's is {client_sha256}. Different hashes mean \
                     the inputs differ; equal ones mean the Argon2d output differs."
                )
            }
            Some("already_used") => anyhow!(
                "Faucet request was unsuccessful: {message}. An identical request is still in \
                 progress; check the balance of {} before requesting again.",
                challenge.recipient
            ),
            Some("stale_checkpoint") => anyhow!(
                "Faucet request was unsuccessful: {message}. Every proof went stale before it \
                 reached the faucet, because solving took longer than the faucet's freshness \
                 window."
            ),
            Some("overloaded" | "funds_unavailable" | "not_ready") => anyhow!(
                "Faucet service is currently unavailable: {message}. Please try again later."
            ),
            _ => anyhow!("Faucet request was unsuccessful: {message}"),
        }
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Requests a payout to `recipient` from the faucet at `base_url`, grinding a proof of work first.
pub(crate) async fn request_gas(base_url: &Url, recipient: SuiAddress) -> anyhow::Result<Payout> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .context("Failed to build the faucet's HTTP client")?;
    let gas_url = base_url.join("v3/gas")?;
    let mut raw = fetch_challenge(&client, base_url, recipient).await?;
    let mut attempt = 1;
    loop {
        let challenge = Challenge::parse(raw, recipient)?;
        let proof = challenge.solve().await?;
        let rejection = match submit_proof(&client, &gas_url, &challenge, &proof).await? {
            Ok(payout) => return Ok(payout),
            Err(rejection) => rejection,
        };

        // A fresh challenge only helps when the proof itself was the problem. In particular,
        // `already_used` can identify a payout that already happened.
        let retryable = matches!(
            rejection.code.as_deref(),
            Some("stale_checkpoint" | "insufficient_work")
        );
        if !retryable || attempt == MAX_PROOF_ATTEMPTS {
            return Err(rejection.into_error(&challenge, &proof));
        }
        attempt += 1;
        eprintln!(
            "The faucet rejected the proof: {}. Solving a new challenge.",
            rejection.message()
        );
        raw = match rejection.challenge {
            Some(fresh) => serde_json::from_value(fresh)
                .context("The faucet returned a challenge this CLI cannot read")?,
            None => fetch_challenge(&client, base_url, recipient).await?,
        };
    }
}

async fn fetch_challenge(
    client: &reqwest::Client,
    base_url: &Url,
    recipient: SuiAddress,
) -> anyhow::Result<RawChallenge> {
    let mut url = base_url.join("v3/challenge")?;
    url.query_pairs_mut()
        .append_pair("recipient", &recipient.to_string());
    let response = client
        .get(url.clone())
        .header(http::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .with_context(|| format!("Failed to reach the faucet at {url}"))?;

    let status = response.status();
    let body = read_body(response)
        .await
        .context("Failed to read the faucet's challenge response")?;
    if status == reqwest::StatusCode::NOT_FOUND {
        bail!(
            "The faucet at {base_url} does not serve proof-of-work challenges. For a faucet that \
             pays without proof of work, pass its full /v2/gas URL."
        );
    }
    if !status.is_success() {
        let rejection = Rejection::parse(status, &body);
        bail!(
            "Failed to get a proof-of-work challenge from the faucet: {}",
            rejection.message()
        );
    }
    serde_json::from_str(&body).context("The faucet returned a challenge this CLI cannot read")
}

async fn submit_proof(
    client: &reqwest::Client,
    gas_url: &Url,
    challenge: &Challenge,
    proof: &Proof,
) -> anyhow::Result<Result<Payout, Rejection>> {
    // Once the request may have reached the faucet, every failure leaves the payout's outcome
    // unknown, since a lost response can hide a payout or a rejection's transaction digest.
    let check_balance = format!(
        "check the balance of {} before requesting again",
        challenge.recipient
    );
    let response = client
        .post(gas_url.clone())
        .header(http::header::USER_AGENT, USER_AGENT)
        .json(&json!({
            "recipient": challenge.recipient.to_string(),
            "checkpointSeq": challenge.checkpoint_seq.to_string(),
            "nonce": proof.nonce.to_string(),
            "hashHex": Hex::encode(proof.hash),
        }))
        .send()
        .await
        .with_context(|| {
            format!(
                "Failed to submit the proof to the faucet. If the request reached it, the payout \
                 may have executed; {check_balance}"
            )
        })?;

    let status = response.status();
    let body = read_body(response).await.with_context(|| {
        format!(
            "Failed to read the faucet's response to the proof. The payout may have executed; \
             {check_balance}"
        )
    })?;
    if status.is_success() {
        let payout = serde_json::from_str(&body).with_context(|| {
            format!(
                "The faucet accepted the proof but returned a response this CLI cannot read: \
                 {body}. The payout has likely executed; {check_balance}"
            )
        })?;
        Ok(Ok(payout))
    } else {
        Ok(Err(Rejection::parse(status, &body)))
    }
}

/// Reads a response body of at most `MAX_RESPONSE_BYTES`, counting the bytes that arrive because
/// a chunked response has no `Content-Length` to check.
async fn read_body(mut response: reqwest::Response) -> anyhow::Result<String> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= MAX_RESPONSE_BYTES,
            "The faucet's response is larger than {MAX_RESPONSE_BYTES} bytes"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// `floor(2^64 / difficulty)`, computed in 128 bits because the numerator does not fit in 64.
fn threshold(difficulty: u64) -> u64 {
    ((1u128 << 64) / u128::from(difficulty.max(MIN_DIFFICULTY))) as u64
}

fn pow_hasher() -> Argon2<'static> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(HASH_LEN),
    )
    .expect("proof-of-work v1 Argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2d, Version::V0x13, params)
}

fn pow_hash(hasher: &Argon2, preimage: &str, blocks: &mut [Block]) -> [u8; HASH_LEN] {
    let mut hash = [0u8; HASH_LEN];
    // Argon2 validates the salt, output, and memory lengths, all fixed here, and a password
    // length limit of 4 GiB that a preimage cannot approach.
    hasher
        .hash_password_into_with_memory(preimage.as_bytes(), POW_SALT.as_bytes(), &mut hash, blocks)
        .expect("proof-of-work v1 Argon2 inputs are valid");
    hash
}

/// The first 8 bytes of the hash as a big-endian integer, which a solution keeps below the
/// threshold.
fn leading_u64(hash: &[u8; HASH_LEN]) -> u64 {
    u64::from_be_bytes(hash[..8].try_into().unwrap())
}

/// Search nonces from `start_nonce` upwards until one hashes below `threshold`, or until
/// `cancelled` is set. Return the winning proof and the number of hashes computed.
fn grind(
    preimage_prefix: &str,
    threshold: u64,
    start_nonce: u64,
    cancelled: &AtomicBool,
) -> Option<(Proof, u64)> {
    let hasher = pow_hasher();
    let mut blocks = vec![Block::default(); hasher.params().block_count()];
    let mut nonce = start_nonce;
    let mut attempts = 0;
    while !cancelled.load(Ordering::Relaxed) {
        let preimage = format!("{preimage_prefix}{nonce}");
        let hash = pow_hash(&hasher, &preimage, &mut blocks);
        attempts += 1;
        if leading_u64(&hash) < threshold {
            return Some((Proof { nonce, hash }, attempts));
        }
        nonce = nonce.wrapping_add(1);
    }
    None
}

/// Formats MIST as SUI, keeping every significant fractional digit.
fn format_sui(mist: u64) -> String {
    let (whole, fraction) = (mist / MIST_PER_SUI, mist % MIST_PER_SUI);
    if fraction == 0 {
        return format!("{whole} SUI");
    }
    let fraction = format!("{fraction:09}");
    format!("{whole}.{} SUI", fraction.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use argon2::Block;
    use fastcrypto::encoding::Encoding;
    use fastcrypto::encoding::Hex;
    use reqwest::Url;
    use sui_types::base_types::SuiAddress;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;
    use tokio::net::TcpListener;

    use super::ARGON2_ALGORITHM;
    use super::ARGON2_ITERATIONS;
    use super::ARGON2_MEMORY_KIB;
    use super::ARGON2_PARALLELISM;
    use super::ARGON2_VERSION;
    use super::Challenge;
    use super::FaucetEndpoint;
    use super::HASH_LEN;
    use super::MAX_RESPONSE_BYTES;
    use super::POW_DOMAIN;
    use super::POW_SALT;
    use super::POW_VERSION;
    use super::Proof;
    use super::RawChallenge;
    use super::format_sui;
    use super::grind;
    use super::leading_u64;
    use super::pow_hash;
    use super::pow_hasher;
    use super::read_body;
    use super::submit_proof;
    use super::threshold;

    // Conformance vectors from the faucet's proof-of-work specification (`pow-vectors.json`),
    // taken from Sui devnet checkpoint 1713342.
    const CHAIN_ID: &str = "8wWZfv1HjQxjB5ncmC9CQqqRqUMBiEZXnc5FhS9zRe1v";
    const CHECKPOINT_SEQ: &str = "1713342";
    const CHECKPOINT_DIGEST: &str = "GzBdc9tbUe3s2PK1iVJcyQBj5TvdX7bJiAHWbhBDYiYb";
    const RANDOM_BYTES: &str = "mJ4ZqDXQasy17GTiGfp0/OYAGCR+7reLM5+Gb39YtoAIaPuh1wj+z5J7po+eSze3";
    const FAUCET_ADDRESS: &str =
        "0x949cd75a2485cc18b8258634ffd9b849a687e2e2a0b4c2444a21f007fc79adc4";
    const RECIPIENT: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    fn raw_challenge(random_bytes: &str, difficulty: &str) -> RawChallenge {
        RawChallenge {
            version: POW_VERSION,
            domain: POW_DOMAIN.to_owned(),
            salt: POW_SALT.to_owned(),
            algorithm: ARGON2_ALGORITHM.to_owned(),
            argon2_version: ARGON2_VERSION,
            memory_size: ARGON2_MEMORY_KIB,
            iterations: ARGON2_ITERATIONS,
            parallelism: ARGON2_PARALLELISM,
            hash_length: HASH_LEN,
            chain_id: CHAIN_ID.to_owned(),
            checkpoint_seq: CHECKPOINT_SEQ.to_owned(),
            checkpoint_digest: CHECKPOINT_DIGEST.to_owned(),
            random_bytes: random_bytes.to_owned(),
            faucet_address: FAUCET_ADDRESS.to_owned(),
            recipient: RECIPIENT.to_owned(),
            difficulty: difficulty.to_owned(),
        }
    }

    fn challenge(random_bytes: &str, difficulty: &str) -> Challenge {
        Challenge::parse(
            raw_challenge(random_bytes, difficulty),
            RECIPIENT.parse().unwrap(),
        )
        .unwrap()
    }

    fn hash(challenge: &Challenge, nonce: u64) -> [u8; HASH_LEN] {
        let hasher = pow_hasher();
        let mut blocks = vec![Block::default(); hasher.params().block_count()];
        pow_hash(&hasher, &challenge.preimage(nonce), &mut blocks)
    }

    /// Answers one connection with `response`, then either holds it open or closes it. The whole
    /// request is read first, because closing a socket with unread input resets the connection.
    async fn serve_once(response: &'static str, hold_open: bool) -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/v3/gas", listener.local_addr().unwrap())).unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = BufReader::new(&mut socket);
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                request.read_line(&mut line).await.unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(len) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = len.trim().parse().unwrap();
                }
            }
            request
                .read_exact(&mut vec![0; content_length])
                .await
                .unwrap();
            socket.write_all(response.as_bytes()).await.unwrap();
            if hold_open {
                std::future::pending::<()>().await;
            }
        });
        url
    }

    #[test]
    fn preimage_matches_vectors() {
        assert_eq!(
            challenge(RANDOM_BYTES, "256").preimage(201),
            "sui-faucet-pow/1\n\
             8wWZfv1HjQxjB5ncmC9CQqqRqUMBiEZXnc5FhS9zRe1v\n\
             1713342\n\
             GzBdc9tbUe3s2PK1iVJcyQBj5TvdX7bJiAHWbhBDYiYb\n\
             mJ4ZqDXQasy17GTiGfp0/OYAGCR+7reLM5+Gb39YtoAIaPuh1wj+z5J7po+eSze3\n\
             0x949cd75a2485cc18b8258634ffd9b849a687e2e2a0b4c2444a21f007fc79adc4\n\
             0x1111111111111111111111111111111111111111111111111111111111111111\n\
             201",
        );
        assert_eq!(
            challenge("", "256").preimage(0),
            "sui-faucet-pow/1\n\
             8wWZfv1HjQxjB5ncmC9CQqqRqUMBiEZXnc5FhS9zRe1v\n\
             1713342\n\
             GzBdc9tbUe3s2PK1iVJcyQBj5TvdX7bJiAHWbhBDYiYb\n\
             \n\
             0x949cd75a2485cc18b8258634ffd9b849a687e2e2a0b4c2444a21f007fc79adc4\n\
             0x1111111111111111111111111111111111111111111111111111111111111111\n\
             0",
        );
    }

    #[test]
    fn hash_matches_vectors() {
        let cases = [
            (
                RANDOM_BYTES,
                0,
                "31f6ec0e9169705f6105219eebbbc77420f1d9273d407db5df17b4d090afb92d",
                3600324499442593887,
            ),
            (
                RANDOM_BYTES,
                42,
                "959c60b9062203b7739fb94aa3577a6173fa36f0cec60e55ce714402b36ffa28",
                10780597955806233527,
            ),
            (
                RANDOM_BYTES,
                201,
                "0008faf113e0b071f5f9d5d6e1dc9321890ecb7e81059f07dedbd3dec5e902bb",
                2527713141239921,
            ),
            (
                RANDOM_BYTES,
                u64::MAX,
                "6f8096ffd12d6c2fe93d8bb12c6b9544e6aa00ed8ee8fcdbe5ebec12000d5dcd",
                8034587760699206703,
            ),
            (
                "",
                0,
                "d8430e5471f4fba3c3c67a253f15dc72588d87c364117e1f138e26387050ec60",
                15583314891483970467,
            ),
        ];
        for (random_bytes, nonce, expected_hash, expected_value) in cases {
            let hash = hash(&challenge(random_bytes, "256"), nonce);
            assert_eq!(Hex::encode(hash), expected_hash, "nonce {nonce}");
            assert_eq!(leading_u64(&hash), expected_value, "nonce {nonce}");
        }
    }

    #[test]
    fn threshold_matches_vectors() {
        let cases = [
            (2, 9223372036854775808),
            (3, 6148914691236517205),
            (7, 2635249153387078802),
            (256, 72057594037927936),
            (724, 25478928278604353),
            (725, 25443784929254553),
            (1000, 18446744073709551),
            (11586, 1592158128233173),
            (281474976710656, 65536),
        ];
        for (difficulty, expected) in cases {
            assert_eq!(threshold(difficulty), expected, "difficulty {difficulty}");
        }
    }

    #[test]
    fn grind_finds_first_solution() {
        // Nonce 201 is the first solution at difficulty 256 when grinding up from zero, so a
        // single thread starting just below it must land on it.
        let challenge = challenge(RANDOM_BYTES, "256");
        let (proof, attempts) = grind(
            &challenge.preimage_prefix,
            threshold(256),
            195,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(proof.nonce, 201);
        assert_eq!(attempts, 7);
        assert_eq!(
            Hex::encode(proof.hash),
            "0008faf113e0b071f5f9d5d6e1dc9321890ecb7e81059f07dedbd3dec5e902bb"
        );
    }

    #[test]
    fn grind_returns_valid_proof() {
        let challenge = challenge(RANDOM_BYTES, "8");
        let (proof, _) = grind(
            &challenge.preimage_prefix,
            threshold(8),
            1000,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(proof.hash, hash(&challenge, proof.nonce));
        assert!(leading_u64(&proof.hash) < threshold(8));
    }

    #[test]
    fn grind_wraps_nonce_without_overflow() {
        let challenge = challenge(RANDOM_BYTES, "4");
        let (proof, attempts) = grind(
            &challenge.preimage_prefix,
            threshold(4),
            u64::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(proof.nonce, 0);
        assert_eq!(attempts, 2);
        assert_eq!(proof.hash, hash(&challenge, 0));
        assert!(leading_u64(&proof.hash) < threshold(4));
    }

    #[test]
    fn grind_stops_when_cancelled() {
        // No hash is below a threshold of 0, so only the flag can end the search.
        let challenge = challenge(RANDOM_BYTES, "256");
        assert!(grind(&challenge.preimage_prefix, 0, 0, &AtomicBool::new(true)).is_none());
    }

    #[test]
    fn rejects_other_protocol_versions() {
        let recipient: SuiAddress = RECIPIENT.parse().unwrap();

        let mut other_version = raw_challenge(RANDOM_BYTES, "256");
        other_version.version = 2;
        assert!(Challenge::parse(other_version, recipient).is_err());

        let mut argon2id = raw_challenge(RANDOM_BYTES, "256");
        argon2id.algorithm = "argon2id".to_owned();
        assert!(Challenge::parse(argon2id, recipient).is_err());

        let mut more_memory = raw_challenge(RANDOM_BYTES, "256");
        more_memory.memory_size = 1 << 20;
        assert!(Challenge::parse(more_memory, recipient).is_err());
    }

    #[test]
    fn rejects_mismatched_recipient_and_bad_difficulty() {
        let other: SuiAddress = FAUCET_ADDRESS.parse().unwrap();
        assert!(Challenge::parse(raw_challenge(RANDOM_BYTES, "256"), other).is_err());

        let recipient: SuiAddress = RECIPIENT.parse().unwrap();
        for difficulty in ["1", "281474976710657", "-5", "1e3", ""] {
            assert!(
                Challenge::parse(raw_challenge(RANDOM_BYTES, difficulty), recipient).is_err(),
                "difficulty {difficulty:?}"
            );
        }
    }

    #[tokio::test]
    async fn read_body_caps_size() {
        let response = |len| reqwest::Response::from(http::Response::new(vec![b'x'; len]));
        assert_eq!(
            read_body(response(MAX_RESPONSE_BYTES)).await.unwrap().len(),
            MAX_RESPONSE_BYTES
        );
        assert!(read_body(response(MAX_RESPONSE_BYTES + 1)).await.is_err());
    }

    #[tokio::test]
    async fn submit_proof_warns_when_response_unreadable() {
        let challenge = challenge(RANDOM_BYTES, "256");
        let proof = Proof {
            nonce: 201,
            hash: [0; HASH_LEN],
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        let cases = [
            (
                "HTTP/1.1 200 OK\r\ncontent-length: 8\r\n\r\nnot json",
                false,
                "The payout has likely executed",
            ),
            (
                "HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n{\"digest\"",
                false,
                "Failed to read the faucet's response",
            ),
            (
                "HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n",
                true,
                "Failed to read the faucet's response",
            ),
        ];
        for (response, hold_open, expected) in cases {
            let gas_url = serve_once(response, hold_open).await;
            let error = submit_proof(&client, &gas_url, &challenge, &proof)
                .await
                .unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains(expected), "{message}");
            assert!(
                message.contains(&format!("check the balance of {RECIPIENT}")),
                "{message}"
            );
        }
    }

    #[test]
    fn parses_endpoints() {
        let pow = |url: &str| FaucetEndpoint::ProofOfWork(Url::parse(url).unwrap());
        let without_pow = |url: &str| FaucetEndpoint::WithoutProofOfWork(url.to_owned());
        let cases = [
            (
                "https://faucet.testnet.sui.io",
                pow("https://faucet.testnet.sui.io/"),
            ),
            (
                "https://faucet.testnet.sui.io/",
                pow("https://faucet.testnet.sui.io/"),
            ),
            (
                "https://faucet.testnet.sui.io/v3/gas",
                pow("https://faucet.testnet.sui.io/"),
            ),
            (
                "http://127.0.0.1:3000/faucet/v3/gas/",
                pow("http://127.0.0.1:3000/faucet/"),
            ),
            (
                "http://127.0.0.1:3000/faucet",
                pow("http://127.0.0.1:3000/faucet/"),
            ),
            (
                "http://127.0.0.1:9123/v2/gas",
                without_pow("http://127.0.0.1:9123/v2/gas"),
            ),
            (
                "http://127.0.0.1:9123/v1/gas",
                without_pow("http://127.0.0.1:9123/v1/gas"),
            ),
        ];
        for (url, expected) in cases {
            assert_eq!(FaucetEndpoint::parse(url).unwrap(), expected, "{url}");
        }
        assert!(FaucetEndpoint::parse("faucet.testnet.sui.io").is_err());
    }

    #[test]
    fn formats_sui() {
        assert_eq!(format_sui(1_000_000_000), "1 SUI");
        assert_eq!(format_sui(1_000_000), "0.001 SUI");
        assert_eq!(format_sui(2_500_000_001), "2.500000001 SUI");
        assert_eq!(format_sui(0), "0 SUI");
    }
}

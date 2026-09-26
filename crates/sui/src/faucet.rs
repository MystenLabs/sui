// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Client for the faucet's proof-of-work API, `GET /v3/challenge` and `POST /v3/gas`.
//!
//! The faucet pays out only after the client spends a server-chosen number of expected Argon2d
//! attempts on a preimage that binds recent public chain state, the faucet, and the recipient.
//! This module implements proof-of-work version 1 (`sui-faucet-pow/1`), which fixes the preimage
//! layout and every hashing parameter below.

use std::time::Instant;

use anyhow::{Context, anyhow, bail, ensure};
use argon2::{Algorithm, Argon2, Block, Params, Version};
use fastcrypto::{
    encoding::{Encoding, Hex},
    hash::{HashFunction, Sha256},
};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use sui_types::{base_types::SuiAddress, gas_coin::MIST_PER_SUI};

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

/// How many proofs one request may grind. A proof is rejected as stale when the chain moves past
/// its freshness window before submission, and the rejection carries a fresh challenge to retry.
const MAX_PROOF_ATTEMPTS: usize = 3;

/// Where `sui client faucet` sends its request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FaucetEndpoint {
    /// A faucet serving the proof-of-work API, identified by its base URL. The path always ends
    /// in `/` so that API paths can be joined onto it.
    Pow(Url),
    /// A `/v1/gas` or `/v2/gas` endpoint that pays without proof of work, such as the local faucet
    /// that `sui start --with-faucet` runs.
    NoPow(String),
}

impl FaucetEndpoint {
    /// Accepts a faucet's base URL, its `/v3/gas` URL, or a `/v1/gas` or `/v2/gas` URL.
    pub(crate) fn parse(url: &str) -> anyhow::Result<Self> {
        let mut parsed = Url::parse(url).with_context(|| format!("Invalid faucet URL: {url}"))?;
        let path = parsed.path().trim_end_matches('/');
        if path.ends_with("/v1/gas") || path.ends_with("/v2/gas") {
            return Ok(Self::NoPow(url.to_owned()));
        }
        let base = format!("{}/", path.strip_suffix("/v3/gas").unwrap_or(path));
        parsed.set_path(&base);
        Ok(Self::Pow(parsed))
    }
}

/// A payout the faucet executed, as reported by `POST /v3/gas`.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Payout {
    pub(crate) digest: String,
    pub(crate) recipient: String,
    amount_mist: String,
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

/// The response of `GET /v3/challenge`, and of the `challenge` field that some rejections carry.
/// u64 values arrive as decimal strings so that JSON cannot round them.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Challenge {
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
struct Puzzle {
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

/// Requests a payout to `recipient` from the faucet at `base`, grinding a proof of work first.
pub(crate) async fn request_gas(base: &Url, recipient: SuiAddress) -> anyhow::Result<Payout> {
    let client = reqwest::Client::new();
    let gas_url = base.join("v3/gas")?;
    let mut challenge = fetch_challenge(&client, base, recipient).await?;
    let mut attempt = 1;
    loop {
        let puzzle = Puzzle::new(challenge, recipient)?;
        let proof = puzzle.solve().await?;
        let rejection = match submit(&client, &gas_url, &puzzle, &proof).await? {
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
            return Err(rejection.into_error(&puzzle, &proof));
        }
        attempt += 1;
        eprintln!(
            "The faucet rejected the proof: {}. Solving a new challenge.",
            rejection.message()
        );
        challenge = match rejection.challenge {
            Some(fresh) => serde_json::from_value(fresh)
                .context("The faucet returned a challenge this CLI cannot read")?,
            None => fetch_challenge(&client, base, recipient).await?,
        };
    }
}

async fn fetch_challenge(
    client: &reqwest::Client,
    base: &Url,
    recipient: SuiAddress,
) -> anyhow::Result<Challenge> {
    let mut url = base.join("v3/challenge")?;
    url.query_pairs_mut()
        .append_pair("recipient", &recipient.to_string());
    let response = client
        .get(url.clone())
        .header(http::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .with_context(|| format!("Failed to reach the faucet at {url}"))?;

    let status = response.status();
    let body = response.text().await?;
    if status == reqwest::StatusCode::NOT_FOUND {
        bail!(
            "The faucet at {base} does not serve proof-of-work challenges. For a faucet that pays \
             without proof of work, pass its full /v2/gas URL."
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

async fn submit(
    client: &reqwest::Client,
    gas_url: &Url,
    puzzle: &Puzzle,
    proof: &Proof,
) -> anyhow::Result<Result<Payout, Rejection>> {
    let response = client
        .post(gas_url.clone())
        .header(http::header::USER_AGENT, USER_AGENT)
        .json(&json!({
            "recipient": puzzle.recipient.to_string(),
            "checkpointSeq": puzzle.checkpoint_seq.to_string(),
            "nonce": proof.nonce.to_string(),
            "hashHex": Hex::encode(proof.hash),
        }))
        .send()
        .await
        .with_context(|| {
            format!(
                "Failed to submit the proof to the faucet. If the request reached it, the payout \
                 may have executed; check the balance of {} before requesting again",
                puzzle.recipient
            )
        })?;

    let status = response.status();
    let body = response.text().await?;
    if status.is_success() {
        let payout = serde_json::from_str(&body)
            .with_context(|| format!("The faucet returned an unexpected response: {body}"))?;
        Ok(Ok(payout))
    } else {
        Ok(Err(Rejection::parse(status, &body)))
    }
}

impl Puzzle {
    fn new(challenge: Challenge, recipient: SuiAddress) -> anyhow::Result<Self> {
        // The spec requires rejecting any other parameters: they would either produce proofs the
        // faucet rejects, or let the faucet choose how much memory and time this client spends.
        ensure!(
            challenge.version == POW_VERSION
                && challenge.domain == POW_DOMAIN
                && challenge.salt == POW_SALT
                && challenge.algorithm == ARGON2_ALGORITHM
                && challenge.argon2_version == ARGON2_VERSION
                && challenge.memory_size == ARGON2_MEMORY_KIB
                && challenge.iterations == ARGON2_ITERATIONS
                && challenge.parallelism == ARGON2_PARALLELISM
                && challenge.hash_length == HASH_LEN,
            "The faucet asks for proof-of-work version {} ({} with m={}, t={}, p={}), but this CLI \
             implements {POW_DOMAIN}. Updating the Sui CLI may fix this.",
            challenge.version,
            challenge.algorithm,
            challenge.memory_size,
            challenge.iterations,
            challenge.parallelism,
        );
        ensure!(
            challenge.recipient == recipient.to_string(),
            "The faucet returned a challenge for recipient {}, not {recipient}",
            challenge.recipient,
        );

        let checkpoint_seq: u64 = challenge
            .checkpoint_seq
            .parse()
            .with_context(|| format!("Invalid checkpointSeq {:?}", challenge.checkpoint_seq))?;
        let difficulty: u64 = challenge
            .difficulty
            .parse()
            .with_context(|| format!("Invalid difficulty {:?}", challenge.difficulty))?;
        ensure!(
            (MIN_DIFFICULTY..=MAX_DIFFICULTY).contains(&difficulty),
            "The faucet asks for difficulty {difficulty}, outside the supported range \
             {MIN_DIFFICULTY} to {MAX_DIFFICULTY}",
        );
        let faucet_address: SuiAddress = challenge
            .faucet_address
            .parse()
            .with_context(|| format!("Invalid faucetAddress {:?}", challenge.faucet_address))?;

        // Addresses and integers are re-rendered in their canonical forms, while the digest and
        // randomness are hashed exactly as the faucet encoded them.
        let preimage_prefix = format!(
            "{POW_DOMAIN}\n{}\n{checkpoint_seq}\n{}\n{}\n{faucet_address}\n{recipient}\n",
            challenge.chain_id, challenge.checkpoint_digest, challenge.random_bytes,
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
        let prefix = self.preimage_prefix.clone();
        let threshold = threshold(self.difficulty);
        let start = rand::random();
        let (proof, attempts) =
            tokio::task::spawn_blocking(move || grind(&prefix, threshold, start)).await?;
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

    fn into_error(self, puzzle: &Puzzle, proof: &Proof) -> anyhow::Error {
        let message = self.message();
        if let Some(digest) = &self.digest {
            return anyhow!(
                "Faucet request was unsuccessful: {message}. The faucet may already have paid out \
                 in transaction {digest}; check that transaction before requesting again."
            );
        }
        match self.code.as_deref() {
            Some("invalid_proof") => {
                let ours = Hex::encode(Sha256::digest(puzzle.preimage(proof.nonce).as_bytes()));
                let theirs = self.preimage_sha256.as_deref().unwrap_or("unknown");
                anyhow!(
                    "Faucet rejected the proof: {message}. SHA-256 of the faucet's preimage is \
                     {theirs}, and of this CLI's is {ours}. Different hashes mean the inputs \
                     differ; equal ones mean the Argon2d output differs."
                )
            }
            Some("already_used") => anyhow!(
                "Faucet request was unsuccessful: {message}. An identical request is still in \
                 progress; check the balance of {} before requesting again.",
                puzzle.recipient
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

/// `floor(2^64 / difficulty)`, computed in 128 bits because the numerator does not fit in 64.
fn threshold(difficulty: u64) -> u64 {
    ((1u128 << 64) / u128::from(difficulty.max(MIN_DIFFICULTY))) as u64
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(HASH_LEN),
    )
    .expect("proof-of-work v1 Argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2d, Version::V0x13, params)
}

fn pow_hash(argon2: &Argon2, preimage: &str, memory: &mut [Block]) -> [u8; HASH_LEN] {
    let mut hash = [0u8; HASH_LEN];
    // Argon2 validates the salt, output, and memory lengths, all fixed here, and a password
    // length limit of 4 GiB that a preimage cannot approach.
    argon2
        .hash_password_into_with_memory(preimage.as_bytes(), POW_SALT.as_bytes(), &mut hash, memory)
        .expect("proof-of-work v1 Argon2 inputs are valid");
    hash
}

/// The first 8 bytes of the hash as a big-endian integer, which a solution keeps below the
/// threshold.
fn hash_value(hash: &[u8; HASH_LEN]) -> u64 {
    u64::from_be_bytes(hash[..8].try_into().unwrap())
}

/// Search nonces from `start` upwards until one hashes below `threshold`.
/// Return the winning proof and the number of hashes computed.
fn grind(preimage_prefix: &str, threshold: u64, start: u64) -> (Proof, u64) {
    let argon2 = argon2();
    let mut memory = vec![Block::default(); argon2.params().block_count()];
    let mut nonce = start;
    let mut attempts = 0;
    loop {
        let preimage = format!("{preimage_prefix}{nonce}");
        let hash = pow_hash(&argon2, &preimage, &mut memory);
        attempts += 1;
        if hash_value(&hash) < threshold {
            return (Proof { nonce, hash }, attempts);
        }
        nonce = nonce.wrapping_add(1);
    }
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
    use super::*;

    // Conformance vectors from the faucet's proof-of-work specification (`pow-vectors.json`),
    // taken from Sui devnet checkpoint 1713342.
    const CHAIN_ID: &str = "8wWZfv1HjQxjB5ncmC9CQqqRqUMBiEZXnc5FhS9zRe1v";
    const CHECKPOINT_SEQ: &str = "1713342";
    const CHECKPOINT_DIGEST: &str = "GzBdc9tbUe3s2PK1iVJcyQBj5TvdX7bJiAHWbhBDYiYb";
    const RANDOM_BYTES: &str = "mJ4ZqDXQasy17GTiGfp0/OYAGCR+7reLM5+Gb39YtoAIaPuh1wj+z5J7po+eSze3";
    const FAUCET_ADDRESS: &str =
        "0x949cd75a2485cc18b8258634ffd9b849a687e2e2a0b4c2444a21f007fc79adc4";
    const RECIPIENT: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    fn challenge(random_bytes: &str, difficulty: &str) -> Challenge {
        Challenge {
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

    fn puzzle(random_bytes: &str, difficulty: &str) -> Puzzle {
        Puzzle::new(
            challenge(random_bytes, difficulty),
            RECIPIENT.parse().unwrap(),
        )
        .unwrap()
    }

    fn hash(puzzle: &Puzzle, nonce: u64) -> [u8; HASH_LEN] {
        let argon2 = argon2();
        let mut memory = vec![Block::default(); argon2.params().block_count()];
        pow_hash(&argon2, &puzzle.preimage(nonce), &mut memory)
    }

    #[test]
    fn preimage_matches_vectors() {
        assert_eq!(
            puzzle(RANDOM_BYTES, "256").preimage(201),
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
            puzzle("", "256").preimage(0),
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
            let hash = hash(&puzzle(random_bytes, "256"), nonce);
            assert_eq!(Hex::encode(hash), expected_hash, "nonce {nonce}");
            assert_eq!(hash_value(&hash), expected_value, "nonce {nonce}");
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
        let puzzle = puzzle(RANDOM_BYTES, "256");
        let (proof, attempts) = grind(&puzzle.preimage_prefix, threshold(256), 195);
        assert_eq!(proof.nonce, 201);
        assert_eq!(attempts, 7);
        assert_eq!(
            Hex::encode(proof.hash),
            "0008faf113e0b071f5f9d5d6e1dc9321890ecb7e81059f07dedbd3dec5e902bb"
        );
    }

    #[test]
    fn grind_returns_valid_proof() {
        let puzzle = puzzle(RANDOM_BYTES, "8");
        let (proof, _) = grind(&puzzle.preimage_prefix, threshold(8), 1000);
        assert_eq!(proof.hash, hash(&puzzle, proof.nonce));
        assert!(hash_value(&proof.hash) < threshold(8));
    }

    #[test]
    fn grind_wraps_nonce_without_overflow() {
        let puzzle = puzzle(RANDOM_BYTES, "4");
        let (proof, attempts) = grind(&puzzle.preimage_prefix, threshold(4), u64::MAX);
        assert_eq!(proof.nonce, 0);
        assert_eq!(attempts, 2);
        assert_eq!(proof.hash, hash(&puzzle, 0));
        assert!(hash_value(&proof.hash) < threshold(4));
    }

    #[test]
    fn rejects_other_protocol_versions() {
        let recipient: SuiAddress = RECIPIENT.parse().unwrap();

        let mut other_version = challenge(RANDOM_BYTES, "256");
        other_version.version = 2;
        assert!(Puzzle::new(other_version, recipient).is_err());

        let mut argon2id = challenge(RANDOM_BYTES, "256");
        argon2id.algorithm = "argon2id".to_owned();
        assert!(Puzzle::new(argon2id, recipient).is_err());

        let mut more_memory = challenge(RANDOM_BYTES, "256");
        more_memory.memory_size = 1 << 20;
        assert!(Puzzle::new(more_memory, recipient).is_err());
    }

    #[test]
    fn rejects_mismatched_recipient_and_bad_difficulty() {
        let other: SuiAddress = FAUCET_ADDRESS.parse().unwrap();
        assert!(Puzzle::new(challenge(RANDOM_BYTES, "256"), other).is_err());

        let recipient: SuiAddress = RECIPIENT.parse().unwrap();
        for difficulty in ["1", "281474976710657", "-5", "1e3", ""] {
            assert!(
                Puzzle::new(challenge(RANDOM_BYTES, difficulty), recipient).is_err(),
                "difficulty {difficulty:?}"
            );
        }
    }

    #[test]
    fn parses_endpoints() {
        let pow = |url: &str| FaucetEndpoint::Pow(Url::parse(url).unwrap());
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
                FaucetEndpoint::NoPow("http://127.0.0.1:9123/v2/gas".to_owned()),
            ),
            (
                "http://127.0.0.1:9123/v1/gas",
                FaucetEndpoint::NoPow("http://127.0.0.1:9123/v1/gas".to_owned()),
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

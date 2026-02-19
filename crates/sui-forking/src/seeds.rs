// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use clap::Args;
use cynic::QueryBuilder;
use tracing::info;

use sui_data_store::{ObjectKey, VersionQuery};
use sui_types::base_types::{ObjectID, SuiAddress};

use crate::store::ForkingStore;

/// Maximum checkpoint age (1h) for account ownership discovery.
pub const OWNED_OBJECT_LOOKBACK_LIMIT_MS: u64 = 3_600_000;

#[derive(Args, Clone, Debug, Default)]
pub struct InitialAccounts {
    /// Addresses whose owned objects should be prefetched at startup.
    ///
    /// Only allowed when the selected startup checkpoint is at most 1 hour old.
    /// Mutually exclusive with `--objects`.
    #[clap(long, value_delimiter = ',', conflicts_with = "objects")]
    pub accounts: Vec<SuiAddress>,

    /// Explicit object IDs to prefetch at startup.
    ///
    /// Use this for older checkpoints where account-owned object history is no longer queryable.
    /// Mutually exclusive with `--accounts`.
    #[clap(long, value_delimiter = ',', conflicts_with = "accounts")]
    pub objects: Vec<ObjectID>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeedMode {
    NoSeed,
    Accounts,
    Objects,
}

impl InitialAccounts {
    /// Resolves the startup prefetch mode from CLI inputs.
    fn seed_mode(&self) -> Result<SeedMode> {
        match (self.accounts.is_empty(), self.objects.is_empty()) {
            (true, true) => Ok(SeedMode::NoSeed),
            (false, true) => Ok(SeedMode::Accounts),
            (true, false) => Ok(SeedMode::Objects),
            (false, false) => {
                anyhow::bail!("`--accounts` and `--objects` cannot be used together")
            }
        }
    }

    /// Returns current UTC wall-clock time in milliseconds.
    fn now_utc_ms() -> u64 {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        duration.as_millis().min(u64::MAX as u128) as u64
    }

    /// Enforces the 1h lookback limit for `--accounts`.
    fn validate_accounts_mode_age(startup_checkpoint: u64, checkpoint_age_ms: u64) -> Result<()> {
        if checkpoint_age_ms <= OWNED_OBJECT_LOOKBACK_LIMIT_MS {
            return Ok(());
        }

        anyhow::bail!(
            "Cannot use --accounts at startup checkpoint {}: checkpoint age is {} ms and exceeds the 1h lookback limit ({} ms). Use --objects with explicit object IDs.",
            startup_checkpoint,
            checkpoint_age_ms,
            OWNED_OBJECT_LOOKBACK_LIMIT_MS
        );
    }

    /// Returns requested object IDs that were not found in the fetched results.
    fn collect_missing_object_ids<T>(
        requested_object_ids: &[ObjectID],
        fetched_objects: &[Option<T>],
    ) -> Vec<ObjectID> {
        requested_object_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, object_id)| {
                if matches!(fetched_objects.get(idx), Some(Some(_))) {
                    None
                } else {
                    Some(*object_id)
                }
            })
            .collect()
    }

    /// Prefetches startup objects according to selected seeding mode.
    ///
    /// `Accounts` mode discovers owned objects through GraphQL and applies the 1h lookback
    /// restriction. `Objects` mode fetches only explicit IDs and fails startup
    /// if any requested object is missing at the startup checkpoint.
    pub async fn prefetch_owned_objects(
        &self,
        store: &ForkingStore,
        graphql_endpoint: &str,
        startup_checkpoint: u64,
        startup_checkpoint_timestamp_ms: u64,
    ) -> Result<()> {
        let seed_mode = self.seed_mode()?;
        info!(
            mode = ?seed_mode,
            account_count = self.accounts.len(),
            object_count = self.objects.len(),
            startup_checkpoint,
            "Selected startup seed mode"
        );

        let requested_object_ids: Vec<ObjectID> = match seed_mode {
            SeedMode::NoSeed => return Ok(()),
            SeedMode::Accounts => {
                let checkpoint_age_ms =
                    Self::now_utc_ms().saturating_sub(startup_checkpoint_timestamp_ms);
                Self::validate_accounts_mode_age(startup_checkpoint, checkpoint_age_ms)?;

                let mut all_object_ids = BTreeSet::new();
                for owner in &self.accounts {
                    info!("Prefetching owned objects for {}", owner);
                    let owned_ids = fetch_owned_object_ids(graphql_endpoint, *owner).await?;
                    info!("Found {} owned object IDs for {}", owned_ids.len(), owner);
                    all_object_ids.extend(owned_ids);
                }

                if all_object_ids.is_empty() {
                    info!("No owned objects found for startup accounts");
                    return Ok(());
                }

                all_object_ids.into_iter().collect()
            }
            SeedMode::Objects => self.objects.clone(),
        };

        if requested_object_ids.is_empty() {
            return Ok(());
        }

        let object_keys: Vec<_> = requested_object_ids
            .into_iter()
            .map(|object_id| ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(startup_checkpoint),
            })
            .collect();

        let fetched_objects = store
            .get_objects(&object_keys)
            .context("Failed to prefetch owned objects from object store")?;

        if seed_mode == SeedMode::Objects {
            let requested_ids = object_keys
                .iter()
                .map(|key| key.object_id)
                .collect::<Vec<_>>();
            let missing_objects =
                Self::collect_missing_object_ids(&requested_ids, &fetched_objects);
            if !missing_objects.is_empty() {
                let missing = missing_objects
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "Failed to prefetch explicit startup objects at checkpoint {}. Missing object IDs: {}",
                    startup_checkpoint,
                    missing
                );
            }
        }

        let fetched = fetched_objects.iter().flatten().count();
        let requested = object_keys.len();
        info!(
            "Startup object prefetch completed at checkpoint {}: fetched {}/{} objects",
            startup_checkpoint, fetched, requested
        );

        Ok(())
    }
}

#[cynic::schema("rpc")]
mod schema {}

#[derive(cynic::QueryVariables, Debug)]
struct AddressVariable {
    address: SuiAddressScalar,
    after: Option<String>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Query", variables = "AddressVariable")]
struct AddressQuery {
    #[arguments(address: $address)]
    address: Option<ObjectsQuery>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Address", variables = "AddressVariable")]
struct ObjectsQuery {
    #[arguments(after: $after)]
    objects: Option<MoveObjectConnection>,
}

#[derive(cynic::QueryFragment, Debug)]
struct MoveObjectConnection {
    edges: Vec<MoveObjectEdge>,
    page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
struct PageInfo {
    end_cursor: Option<String>,
    has_next_page: bool,
}

#[derive(cynic::QueryFragment, Debug)]
struct MoveObjectEdge {
    node: MoveObject,
}

#[derive(cynic::QueryFragment, Debug)]
struct MoveObject {
    address: SuiAddressScalar,
}

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(graphql_type = "SuiAddress")]
struct SuiAddressScalar(String);

/// Fetch all owned object IDs for an address with GraphQL pagination.
async fn fetch_owned_object_ids(
    graphql_endpoint: &str,
    address: SuiAddress,
) -> Result<Vec<ObjectID>> {
    let client = reqwest::Client::new();
    let mut all_object_ids = Vec::new();
    let mut cursor: Option<String> = None;
    let mut has_next_page = true;

    while has_next_page {
        let query = AddressQuery::build(AddressVariable {
            after: cursor.clone(),
            address: SuiAddressScalar(address.to_string()),
        });

        let response = client
            .post(graphql_endpoint)
            .json(&query)
            .send()
            .await
            .context("Failed to send GraphQL request for owned objects")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            anyhow::bail!(
                "GraphQL owned objects request failed with status {}: {}",
                status,
                error_text
            );
        }

        let graphql_response: cynic::GraphQlResponse<AddressQuery> = response
            .json()
            .await
            .context("Failed to parse GraphQL response for owned objects")?;

        if let Some(errors) = &graphql_response.errors {
            let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            anyhow::bail!(
                "GraphQL errors while loading owned objects: {}",
                messages.join(", ")
            );
        }

        let data = graphql_response
            .data
            .ok_or_else(|| anyhow!("No data in GraphQL owned objects response"))?;
        let address_data = data
            .address
            .ok_or_else(|| anyhow!("Address not found in GraphQL owned objects response"))?;
        let objects = address_data
            .objects
            .ok_or_else(|| anyhow!("Owned objects connection missing in GraphQL response"))?;

        for edge in objects.edges {
            let object_id = ObjectID::from_hex_literal(&edge.node.address.0)
                .context("Failed to parse object ID from GraphQL owned objects response")?;
            all_object_ids.push(object_id);
        }

        has_next_page = objects.page_info.has_next_page;
        cursor = objects.page_info.end_cursor;
    }

    Ok(all_object_ids)
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use sui_types::base_types::{ObjectID, SuiAddress};

    use super::{InitialAccounts, OWNED_OBJECT_LOOKBACK_LIMIT_MS, SeedMode};

    #[derive(Parser, Debug)]
    struct SeedCli {
        #[clap(flatten)]
        seeds: InitialAccounts,
    }

    fn parse_address(value: &str) -> SuiAddress {
        SuiAddress::from_str(value).expect("valid address")
    }

    fn parse_object_id(value: &str) -> ObjectID {
        ObjectID::from_hex_literal(value).expect("valid object id")
    }

    use std::str::FromStr;

    #[test]
    fn resolves_seed_mode() {
        let no_seed = InitialAccounts::default();
        assert_eq!(no_seed.seed_mode().expect("mode"), SeedMode::NoSeed);

        let address =
            parse_address("0x0000000000000000000000000000000000000000000000000000000000000002");
        let accounts = InitialAccounts {
            accounts: vec![address],
            objects: vec![],
        };
        assert_eq!(accounts.seed_mode().expect("mode"), SeedMode::Accounts);

        let objects = InitialAccounts {
            accounts: vec![],
            objects: vec![parse_object_id("0x5")],
        };
        assert_eq!(objects.seed_mode().expect("mode"), SeedMode::Objects);
    }

    #[test]
    fn rejects_accounts_mode_for_old_checkpoint() {
        let err =
            InitialAccounts::validate_accounts_mode_age(100, OWNED_OBJECT_LOOKBACK_LIMIT_MS + 1)
                .expect_err("old checkpoint must fail for accounts mode");
        assert!(err.to_string().contains("--objects"));
    }

    #[test]
    fn allows_accounts_mode_at_or_within_limit() {
        InitialAccounts::validate_accounts_mode_age(100, OWNED_OBJECT_LOOKBACK_LIMIT_MS)
            .expect("exactly at limit should pass");
        InitialAccounts::validate_accounts_mode_age(100, OWNED_OBJECT_LOOKBACK_LIMIT_MS - 1)
            .expect("below limit should pass");
    }

    #[test]
    fn rejects_accounts_mode_when_old_even_if_resume() {
        InitialAccounts::validate_accounts_mode_age(100, OWNED_OBJECT_LOOKBACK_LIMIT_MS + 10_000)
            .expect_err("old checkpoint should fail regardless of fresh/bootstrap state");
    }

    #[test]
    fn collects_missing_object_ids() {
        let id1 = parse_object_id("0x11");
        let id2 = parse_object_id("0x22");
        let id3 = parse_object_id("0x33");
        let requested = vec![id1, id2, id3];
        let fetched = vec![Some(()), None, Some(())];
        let missing = InitialAccounts::collect_missing_object_ids(&requested, &fetched);
        assert_eq!(missing, vec![id2]);
    }

    #[test]
    fn clap_rejects_accounts_objects_conflict() {
        let parse_result = SeedCli::try_parse_from([
            "seed-cli",
            "--accounts",
            "0x0000000000000000000000000000000000000000000000000000000000000002",
            "--objects",
            "0x5",
        ]);
        assert!(parse_result.is_err());
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Display, Formatter},
    ops::{Index, IndexMut},
};

use mysten_network::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::{AuthorityName, NetworkPublicKey, ProtocolPublicKey};

/// Committee of the consensus protocol is updated each epoch.
pub type Epoch = u64;

/// Voting power of an authority, roughly proportional to the actual amount of Sui staked
/// by the authority.
/// Total stake / voting power of all authorities should sum to 10,000.
pub type Stake = u64;

/// Committee is the set of authorities that participate in the consensus protocol for this epoch.
/// Its configuration is stored and computed on chain.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Committee {
    /// The epoch number of this committee
    epoch: Epoch,
    /// Protocol and network info of each authority.
    authorities: Vec<Authority>,
    /// Total stakes in the committee.
    total_stake: Stake,

    /// Thresholds related to different fault tolerances.
    quorum_threshold: Stake,
    certification_threshold: Stake,
    validity_threshold: Stake,
}

impl Committee {
    pub fn new(epoch: Epoch, authorities: Vec<Authority>) -> Self {
        assert!(!authorities.is_empty(), "Committee cannot be empty!");
        assert!(
            authorities.len() < u32::MAX as usize,
            "Too many authorities ({})!",
            authorities.len()
        );

        let total_stake: Stake = authorities.iter().map(|a| a.stake).sum();
        assert_ne!(total_stake, 0, "Total stake cannot be zero!");

        // Tolerate integer f faults when total stake is 3f+1.
        let fault_tolerance = (total_stake - 1) / 3;
        let quorum_threshold = total_stake - fault_tolerance;
        let validity_threshold = fault_tolerance + 1;
        assert!(
            2 * quorum_threshold - fault_tolerance > total_stake,
            "Quorum must intersect under maxim equivocations! Quorum: {quorum_threshold}, Fault tolerance: {fault_tolerance}, Total: {total_stake}"
        );

        Self {
            epoch,
            authorities,
            total_stake,
            quorum_threshold,
            validity_threshold,

            // Equivalent to quorum_threshold in v2, and unused anyway.
            certification_threshold: quorum_threshold,
        }
    }

    /// Constructs a committee with thresholds derived from a hybrid fault budget
    /// (`malicious_stake = f`, `crash_stake = c`).
    ///
    /// Nominally, the total stake is `nominal_total_stake = 5f + 3c + 1`;
    /// and the thresholds evaluate to:
    ///
    /// - `validity_threshold       = f + 1`
    /// - `certification_threshold  = 2f + c + 1`
    /// - `quorum_threshold         = 4f + 2c + 1`
    ///
    /// But the actual total stakes specified by the authorities may differ
    /// from the nominal total stake computed above. We will scale `f` and `c`
    /// from the nominal value to the largest possible values where intersection
    /// properties still hold. Then the thresholds are computed with scaled `f` and `c`.
    pub fn new_v3(
        epoch: Epoch,
        authorities: Vec<Authority>,
        malicious_stake: Stake,
        crash_stake: Stake,
    ) -> Self {
        assert!(!authorities.is_empty(), "Committee cannot be empty!");
        assert!(
            authorities.len() < u32::MAX as usize,
            "Too many authorities ({})!",
            authorities.len()
        );

        let actual_total_stake: Stake = authorities.iter().map(|a| a.stake).sum();
        assert_ne!(actual_total_stake, 0, "Total stake cannot be zero!");

        // Compute v3 thresholds.
        let base_stake = 5 * malicious_stake + 3 * crash_stake;
        let (f, c) = if base_stake > 0 {
            // Scale malicious and crash stakes to the real committee stake.
            // Use truncating division to get realistic fault budgets.
            let scale = |nominal: Stake| -> Stake {
                nominal
                    .checked_mul(actual_total_stake - 1)
                    .unwrap_or_else(|| panic!("Overflowed: {} {}", nominal, actual_total_stake - 1))
                    .checked_div(base_stake)
                    .unwrap_or_else(|| panic!("Division error: {} {}", nominal, base_stake))
            };
            (scale(malicious_stake), scale(crash_stake))
        } else {
            // If both fault budgets are zero, there's nothing to scale.
            (0, 0)
        };

        let validity_threshold = f + 1;
        let certification_threshold = 2 * f + c + 1;
        let quorum_threshold = actual_total_stake - f - c;

        // Ensure intersection between committed certification and quorum thresholds.
        assert!(
            certification_threshold + quorum_threshold >= actual_total_stake + f + 1,
            "Stake-safety invariant violated: \
                committed_cert ({certification_threshold}) + \
                quorum ({quorum_threshold}) < \
                actual_total_stake ({actual_total_stake}) + f ({f}) + 1"
        );

        // Ensure a committed certificate survives with the intersection between quorum thresholds.
        assert!(
            quorum_threshold * 2 >= actual_total_stake + f + certification_threshold,
            "Stake-safety invariant violated: \
                quorum_threshold ({quorum_threshold}) * 2 < \
                actual_total_stake ({actual_total_stake}) + f ({f}) + \
                certification_threshold ({certification_threshold})"
        );

        Self {
            epoch,
            total_stake: actual_total_stake,
            quorum_threshold,
            certification_threshold,
            validity_threshold,
            authorities,
        }
    }

    // -----------------------------------------------------------------------
    // Accessors to Committee fields.

    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    pub fn total_stake(&self) -> Stake {
        self.total_stake
    }

    pub fn quorum_threshold(&self) -> Stake {
        self.quorum_threshold
    }

    pub fn certification_threshold(&self) -> Stake {
        self.certification_threshold
    }

    pub fn validity_threshold(&self) -> Stake {
        self.validity_threshold
    }

    pub fn stake(&self, authority_index: AuthorityIndex) -> Stake {
        self.authorities[authority_index].stake
    }

    pub fn authority(&self, authority_index: AuthorityIndex) -> &Authority {
        &self.authorities[authority_index]
    }

    pub fn authorities(&self) -> impl Iterator<Item = (AuthorityIndex, &Authority)> {
        self.authorities
            .iter()
            .enumerate()
            .map(|(i, a)| (AuthorityIndex(i as u32), a))
    }

    /// Returns the authorities as a slice, preserving their order (and hence
    /// their `AuthorityIndex` values). Useful for rebuilding a `Committee` with
    /// different threshold parameters while keeping the same authority set.
    pub fn authorities_slice(&self) -> &[Authority] {
        &self.authorities
    }

    // -----------------------------------------------------------------------
    // Helpers for Committee properties.

    /// Returns true if the provided stake has reached quorum (2f+1).
    pub fn reached_quorum(&self, stake: Stake) -> bool {
        stake >= self.quorum_threshold()
    }

    /// Returns true if the provided stake has reached validity (f+1).
    pub fn reached_validity(&self, stake: Stake) -> bool {
        stake >= self.validity_threshold()
    }

    /// Converts an index to an AuthorityIndex, if valid.
    /// Returns None if index is out of bound.
    pub fn to_authority_index(&self, index: usize) -> Option<AuthorityIndex> {
        if index < self.authorities.len() {
            Some(AuthorityIndex(index as u32))
        } else {
            None
        }
    }

    /// Returns true if the provided index is valid.
    pub fn is_valid_index(&self, index: AuthorityIndex) -> bool {
        index.value() < self.size()
    }

    /// Returns number of authorities in the committee.
    pub fn size(&self) -> usize {
        self.authorities.len()
    }
}

/// Represents one authority in the committee.
///
/// NOTE: this is intentionally un-cloneable, to encourage only copying relevant fields.
/// AuthorityIndex should be used to reference an authority instead.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Authority {
    /// Voting power of the authority in the committee.
    pub stake: Stake,
    /// Network address for communicating with the authority.
    pub address: Multiaddr,
    /// The authority's hostname, for metrics and logging.
    pub hostname: String,
    /// The authority's name, matching AuthorityName on the Sui side.
    pub authority_name: AuthorityName,
    /// The authority's public key for verifying blocks.
    pub protocol_key: ProtocolPublicKey,
    /// The authority's public key for TLS and as network identity.
    pub network_key: NetworkPublicKey,
}

/// Each authority is uniquely identified by its AuthorityIndex in the Committee.
/// AuthorityIndex is between 0 (inclusive) and the total number of authorities (exclusive).
///
/// NOTE: for safety, invalid AuthorityIndex should be impossible to create. So AuthorityIndex
/// should not be created or incremented outside of this file. AuthorityIndex received from peers
/// should be validated before use.
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Debug, Default, Hash, Serialize, Deserialize,
)]
pub struct AuthorityIndex(u32);

impl AuthorityIndex {
    // Minimum committee size is 1, so 0 index is always valid.
    pub const ZERO: Self = Self(0);

    // Only for scanning rows in the database. Invalid elsewhere.
    pub const MIN: Self = Self::ZERO;
    pub const MAX: Self = Self(u32::MAX);

    pub fn value(&self) -> usize {
        self.0 as usize
    }
}

impl AuthorityIndex {
    pub fn new_for_test(index: u32) -> Self {
        Self(index)
    }
}

impl Display for AuthorityIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.value())
    }
}

impl<T, const N: usize> Index<AuthorityIndex> for [T; N] {
    type Output = T;

    fn index(&self, index: AuthorityIndex) -> &Self::Output {
        self.get(index.value()).unwrap()
    }
}

impl<T> Index<AuthorityIndex> for Vec<T> {
    type Output = T;

    fn index(&self, index: AuthorityIndex) -> &Self::Output {
        self.get(index.value()).unwrap()
    }
}

impl<T, const N: usize> IndexMut<AuthorityIndex> for [T; N] {
    fn index_mut(&mut self, index: AuthorityIndex) -> &mut Self::Output {
        self.get_mut(index.value()).unwrap()
    }
}

impl<T> IndexMut<AuthorityIndex> for Vec<T> {
    fn index_mut(&mut self, index: AuthorityIndex) -> &mut Self::Output {
        self.get_mut(index.value()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{local_committee_and_keys, local_committee_and_keys_with_test_options};

    #[test]
    fn committee_basic() {
        // GIVEN
        let epoch = 100;
        let num_of_authorities = 10;
        let authority_stakes = (1..=num_of_authorities).map(|s| s as Stake).collect();
        let (committee, _) = local_committee_and_keys(epoch, authority_stakes);

        // THEN make sure the output Committee fields are populated correctly.
        assert_eq!(committee.size(), num_of_authorities);
        for (i, authority) in committee.authorities() {
            assert_eq!((i.value() + 1) as Stake, authority.stake);
        }

        // AND ensure thresholds are calculated correctly.
        assert_eq!(committee.total_stake(), 55);
        assert_eq!(committee.quorum_threshold(), 37);
        assert_eq!(committee.validity_threshold(), 19);
    }

    #[test]
    fn committee_thresholds_across_sizes() {
        struct Case {
            n: usize,
            stake: Stake,
            total: Stake,
            quorum: Stake,
            validity: Stake,
        }
        let cases = [
            Case {
                n: 11,
                stake: 1,
                total: 11,
                quorum: 8,
                validity: 4,
            },
            Case {
                n: 12,
                stake: 10,
                total: 120,
                quorum: 81,
                validity: 40,
            },
        ];

        for case in cases {
            let stakes = vec![case.stake; case.n];
            let (committee, _) = local_committee_and_keys(100, stakes);
            assert_eq!(committee.total_stake(), case.total);
            assert_eq!(committee.quorum_threshold(), case.quorum);
            assert_eq!(committee.validity_threshold(), case.validity);
        }
    }

    fn create_committee_with_total_stake(num_authorities: usize, total_stake: Stake) -> Committee {
        // Spreads `total_stake` across `num_authorities` (sandbox-safe addresses).
        // The last authority absorbs the remainder so the sum is exact.
        assert!(num_authorities > 0);
        let per = total_stake / num_authorities as Stake;
        let mut stakes = vec![per; num_authorities];
        *stakes.last_mut().unwrap() = total_stake - per * (num_authorities as Stake - 1);
        let (committee, _) = local_committee_and_keys_with_test_options(0, stakes, false);
        assert_eq!(committee.total_stake(), total_stake);
        committee
    }

    #[test]
    fn committee_v3_thresholds_across_actual_stakes() {
        // Thresholds follow:
        //   f_scaled = floor(f_nominal * (actual - 1) / (5f + 3c))
        //   c_scaled likewise
        //   validity      = f_scaled + 1
        //   certification = 2 * f_scaled + c_scaled + 1
        //   quorum        = actual - f_scaled - c_scaled
        // The formulas depend on total stake and the nominal f, c — not on the
        // number of authorities, so cases below also vary `num_authorities`.
        // `new_v3` asserts stake-safety invariants internally, so each case
        // below exercises those invariants too.
        struct Case {
            name: &'static str,
            num_authorities: usize,
            actual: Stake,
            malicious: Stake,
            crash: Stake,
            validity: Stake,
            cert: Stake,
            quorum: Stake,
        }
        let cases = [
            // Actual == nominal budget (5f + 3c + 1 with f=c=1250): no scaling.
            Case {
                name: "no scaling",
                num_authorities: 4,
                actual: 10_001,
                malicious: 1_250,
                crash: 1_250,
                validity: 1_251,
                cert: 3_751,
                quorum: 7_501,
            },
            // Tight boundary: actual == nominal + 1, truncation keeps f and c
            // at the nominal values.
            Case {
                name: "tight boundary",
                num_authorities: 7,
                actual: 10_002,
                malicious: 1_250,
                crash: 1_250,
                validity: 1_251,
                cert: 3_751,
                quorum: 7_502,
            },
            // Non-integer scale factor.
            Case {
                name: "scale with remainder",
                num_authorities: 10,
                actual: 15_000,
                malicious: 1_250,
                crash: 1_250,
                validity: 1_875,
                cert: 5_623,
                quorum: 11_252,
            },
            // Aggressive scaling: tiny nominal f=c=1 with large actual stake
            // forces f_scaled = c_scaled = 2500.
            Case {
                name: "aggressive scaling",
                num_authorities: 5,
                actual: 20_002,
                malicious: 1,
                crash: 1,
                validity: 2_501,
                cert: 7_501,
                quorum: 15_002,
            },
            // Crash-only: f_nominal=0 ⇒ f_scaled=0; only crash faults scaled.
            Case {
                name: "crash-only (f=0)",
                num_authorities: 4,
                actual: 10_000,
                malicious: 0,
                crash: 1_000,
                validity: 1,
                cert: 3_334,
                quorum: 6_667,
            },
            // Byzantine-only: c_nominal=0 ⇒ c_scaled=0; only malicious faults
            // scaled.
            Case {
                name: "byzantine-only (c=0)",
                num_authorities: 6,
                actual: 10_000,
                malicious: 1_000,
                crash: 0,
                validity: 2_000,
                cert: 3_999,
                quorum: 8_001,
            },
        ];

        for case in cases {
            let seed = create_committee_with_total_stake(case.num_authorities, case.actual);
            let committee = Committee::new_v3(
                seed.epoch(),
                seed.authorities_slice().to_vec(),
                case.malicious,
                case.crash,
            );
            assert_eq!(committee.size(), case.num_authorities, "{}", case.name);
            assert_eq!(committee.total_stake(), case.actual, "{}", case.name);
            assert_eq!(
                committee.validity_threshold(),
                case.validity,
                "{}",
                case.name
            );
            assert_eq!(
                committee.certification_threshold(),
                case.cert,
                "{}",
                case.name
            );
            assert_eq!(committee.quorum_threshold(), case.quorum, "{}", case.name);
        }
    }

    #[test]
    fn committee_v3_no_fault_budget_single_authority() {
        // f=c=0: single trusted authority, all thresholds collapse to 1.
        let (seed, _) = local_committee_and_keys_with_test_options(0, vec![100 as Stake], false);
        let committee = Committee::new_v3(seed.epoch(), seed.authorities_slice().to_vec(), 0, 0);
        assert_eq!(committee.validity_threshold(), 1);
        assert_eq!(committee.certification_threshold(), 1);
        assert_eq!(committee.quorum_threshold(), 100);
    }

    #[test]
    #[should_panic(expected = "Total stake cannot be zero!")]
    fn committee_v3_zero_actual_stake_panics() {
        let zero_stake_authorities: Vec<Authority> = {
            let (seed, _) =
                local_committee_and_keys_with_test_options(0, vec![1 as Stake; 4], false);
            seed.authorities_slice()
                .iter()
                .map(|a| Authority {
                    stake: 0,
                    ..a.clone()
                })
                .collect()
        };
        Committee::new_v3(0, zero_stake_authorities, 1_250, 1_250);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    committee::{Committee, StakeUnit},
    error::SuiError,
};
use tracing::{debug, warn};

use crate::stake_aggregator::{InsertResult, StakeAggregator};

/// Tracks error categorization and retry state for a specific operation type
#[derive(Debug)]
pub struct ErrorCategorizer {
    committee: Arc<Committee>,
    // Track non-retryable errors by stake weight
    non_retryable_aggregator: StakeAggregator<(), true>,
    // Track retryable errors by stake weight
    retryable_aggregator: StakeAggregator<(), true>,
    // Track all errors for reporting
    errors: Vec<(SuiError, Vec<AuthorityName>, StakeUnit)>,
    // Track validators that have responded (to avoid double-counting)
    responded_validators: HashMap<AuthorityName, bool>,
}

impl ErrorCategorizer {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            committee,
            non_retryable_aggregator: StakeAggregator::new(committee.clone()),
            retryable_aggregator: StakeAggregator::new(committee.clone()),
            errors: Vec::new(),
            responded_validators: HashMap::new(),
        }
    }

    /// Record an error from a validator and categorize it
    pub fn record_error(
        &mut self,
        authority: AuthorityName,
        error: SuiError,
    ) -> ErrorCategorizationResult {
        let weight = self.committee.weight(&authority);

        // Check if we've already recorded a response from this validator
        if self.responded_validators.contains_key(&authority) {
            debug!(
                "Validator {} already responded, ignoring duplicate error",
                authority.concise()
            );
            return ErrorCategorizationResult::Continue;
        }

        // Mark this validator as having responded
        self.responded_validators.insert(authority, false);

        // Categorize the error
        let (retryable, categorized) = error.is_retryable();

        if !categorized {
            warn!(
                "Uncategorized error from {}: {:?}",
                authority.concise(),
                error
            );
        }

        // Record the error
        self.errors.push((error.clone(), vec![authority], weight));

        if retryable {
            // Add to retryable aggregator
            match self.retryable_aggregator.insert_generic(authority, ()) {
                InsertResult::QuorumReached(_) => {
                    debug!("Retryable errors reached quorum threshold");
                    ErrorCategorizationResult::RetryableQuorumReached
                }
                InsertResult::NotEnoughVotes { .. } => {
                    // Check if we have f+1 non-retryable errors (fatal condition)
                    if self.non_retryable_aggregator.total_votes()
                        >= self.committee.validity_threshold()
                    {
                        ErrorCategorizationResult::FatalQuorumReached
                    } else {
                        ErrorCategorizationResult::Continue
                    }
                }
                InsertResult::Failed { error } => {
                    warn!("Failed to insert retryable error: {:?}", error);
                    ErrorCategorizationResult::Continue
                }
            }
        } else {
            // Add to non-retryable aggregator
            match self.non_retryable_aggregator.insert_generic(authority, ()) {
                InsertResult::QuorumReached(_) => {
                    debug!("Non-retryable errors reached quorum threshold");
                    ErrorCategorizationResult::FatalQuorumReached
                }
                InsertResult::NotEnoughVotes { .. } => {
                    // Check if we have f+1 non-retryable errors (fatal condition)
                    if self.non_retryable_aggregator.total_votes()
                        >= self.committee.validity_threshold()
                    {
                        ErrorCategorizationResult::FatalQuorumReached
                    } else {
                        ErrorCategorizationResult::Continue
                    }
                }
                InsertResult::Failed { error } => {
                    warn!("Failed to insert non-retryable error: {:?}", error);
                    ErrorCategorizationResult::Continue
                }
            }
        }
    }

    /// Record a successful response from a validator
    pub fn record_success(&mut self, authority: AuthorityName) {
        // Mark this validator as having responded successfully
        self.responded_validators.insert(authority, true);
    }

    /// Check if we should continue retrying based on current state
    pub fn should_continue_retrying(&self) -> bool {
        let total_responded_stake: StakeUnit = self
            .responded_validators
            .iter()
            .map(|(authority, _)| self.committee.weight(authority))
            .sum();

        let non_retryable_stake = self.non_retryable_aggregator.total_votes();
        let retryable_stake = self.retryable_aggregator.total_votes();

        // If we have f+1 non-retryable errors, we can't reach quorum
        if non_retryable_stake >= self.committee.validity_threshold() {
            return false;
        }

        // If we have enough total stake to potentially reach quorum, continue
        let potential_quorum_stake = total_responded_stake + retryable_stake;
        potential_quorum_stake >= self.committee.quorum_threshold()
    }

    /// Get the current error state for reporting
    pub fn get_error_state(&self) -> ErrorState {
        let non_retryable_stake = self.non_retryable_aggregator.total_votes();
        let retryable_stake = self.retryable_aggregator.total_votes();
        let total_responded_stake: StakeUnit = self
            .responded_validators
            .iter()
            .map(|(authority, _)| self.committee.weight(authority))
            .sum();

        ErrorState {
            non_retryable_stake,
            retryable_stake,
            total_responded_stake,
            errors: self.errors.clone(),
            should_continue: self.should_continue_retrying(),
        }
    }

    /// Reset the categorizer for a new operation
    pub fn reset(&mut self) {
        self.non_retryable_aggregator = StakeAggregator::new(self.committee.clone());
        self.retryable_aggregator = StakeAggregator::new(self.committee.clone());
        self.errors.clear();
        self.responded_validators.clear();
    }
}

/// Result of error categorization
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCategorizationResult {
    /// Continue retrying - no fatal condition reached
    Continue,
    /// Retryable errors reached quorum threshold
    RetryableQuorumReached,
    /// Non-retryable errors reached quorum threshold (fatal)
    FatalQuorumReached,
}

/// Current error state for reporting
#[derive(Debug, Clone)]
pub struct ErrorState {
    pub non_retryable_stake: StakeUnit,
    pub retryable_stake: StakeUnit,
    pub total_responded_stake: StakeUnit,
    pub errors: Vec<(SuiError, Vec<AuthorityName>, StakeUnit)>,
    pub should_continue: bool,
}

impl ErrorState {
    /// Check if we have enough non-retryable errors to prevent quorum
    pub fn has_fatal_errors(&self) -> bool {
        self.non_retryable_stake > 0
    }

    /// Get a summary of the error state
    pub fn summary(&self) -> String {
        format!(
            "Non-retryable stake: {}, Retryable stake: {}, Total responded: {}, Should continue: {}",
            self.non_retryable_stake,
            self.retryable_stake,
            self.total_responded_stake,
            self.should_continue
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::committee::Committee;
    use sui_types::crypto::{get_authority_key_pair, AuthorityKeyPair};
    use sui_types::error::{SuiError, UserInputError};

    fn create_test_committee() -> Arc<Committee> {
        let mut authorities = Vec::new();
        for i in 0..4 {
            let (_, authority_key) = get_authority_key_pair();
            authorities.push((authority_key.public(), 1));
        }
        Arc::new(Committee::new_for_testing(0, authorities))
    }

    #[test]
    fn test_error_categorization() {
        let committee = create_test_committee();
        let mut categorizer = ErrorCategorizer::new(committee.clone());

        // Test retryable error
        let retryable_error = SuiError::RpcError {
            error: "Network error".to_string(),
        };
        let authority1 = AuthorityName::from([1u8; 32]);

        let result = categorizer.record_error(authority1, retryable_error);
        assert_eq!(result, ErrorCategorizationResult::Continue);

        // Test non-retryable error
        let non_retryable_error = SuiError::ExecutionError("Execution failed".to_string());
        let authority2 = AuthorityName::from([2u8; 32]);

        let result = categorizer.record_error(authority2, non_retryable_error);
        assert_eq!(result, ErrorCategorizationResult::Continue);

        // Test success recording
        let authority3 = AuthorityName::from([3u8; 32]);
        categorizer.record_success(authority3);

        let state = categorizer.get_error_state();
        assert_eq!(state.non_retryable_stake, 1);
        assert_eq!(state.retryable_stake, 1);
        assert_eq!(state.total_responded_stake, 3);
    }

    #[test]
    fn test_fatal_condition() {
        let committee = create_test_committee();
        let mut categorizer = ErrorCategorizer::new(committee.clone());

        // Add f+1 non-retryable errors (f+1 = 2 for 4 validators)
        let authority1 = AuthorityName::from([1u8; 32]);
        let authority2 = AuthorityName::from([2u8; 32]);

        let non_retryable_error = SuiError::ExecutionError("Execution failed".to_string());

        categorizer.record_error(authority1, non_retryable_error.clone());
        let result = categorizer.record_error(authority2, non_retryable_error);

        // Should reach fatal condition with f+1 non-retryable errors
        assert_eq!(result, ErrorCategorizationResult::FatalQuorumReached);

        let state = categorizer.get_error_state();
        assert!(!state.should_continue);
    }
}

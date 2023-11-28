// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a Rule for the Closed Loop Token which sets the Policy for a
/// limited number of epochs. Works like a timelock but uses epochs as the unit.
///
/// The epoch setting is inclusive, i.e. the request is valid before the set epoch.
/// Example: if epoch is 3, the request is valid until the end of epoch 2.
module examples::before_epoch_rule {
    use sui::tx_context::{epoch, TxContext};
    use sui::token::{
        Self,
        TokenPolicy,
        TokenPolicyCap,
        ActionRequest
    };

    /// The Configuration for the rule is missing.
    const ENotConfigured: u64 = 0;
    /// Current epoch is too late for the request.
    const EEpochTooLate: u64 = 1;

    /// The Rule witness.
    struct BeforeEpoch has drop {}

    /// The Rule config.
    struct Config has store, drop {
        epoch: u64
    }

    /// Verifies that the current epoch is not later than the set epoch.
    /// Aborts if the config is not set or if the current epoch is too late.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext
    ) {
        assert!(token::has_rule_config<T, BeforeEpoch>(policy), ENotConfigured);

        let config: &Config = token::rule_config(BeforeEpoch {}, policy);
        let before_epoch = config.epoch;

        assert!(epoch(ctx) < before_epoch, EEpochTooLate);
        token::add_approval(BeforeEpoch {}, request, ctx);
    }

    /// Sets the epoch for the rule. Configurable - can be changed at any time
    /// if the Policy supports mutability (not frozen).
    public fun set_epoch<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        epoch: u64,
        ctx: &mut TxContext
    ) {
        // if there's no stored config for the rule, add a new one
        if (!token::has_rule_config<T, BeforeEpoch>(policy)) {
            let config = Config { epoch };
            token::add_rule_config(BeforeEpoch {}, policy, cap, config, ctx);
        } else {
            let config: &mut Config = token::rule_config_mut(
                BeforeEpoch {}, policy, cap
            );

            config.epoch = epoch;
        }
    }
}

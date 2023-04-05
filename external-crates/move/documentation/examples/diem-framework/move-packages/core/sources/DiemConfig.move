/// Publishes configuration information for validators, and issues reconfiguration events
/// to synchronize configuration changes for the validators.
module CoreFramework::DiemConfig {
    use std::errors;
    use std::event;
    use std::signer;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    friend CoreFramework::Account;
    friend CoreFramework::DiemConsensusConfig;
    friend CoreFramework::DiemSystem;
    friend CoreFramework::DiemVersion;
    friend CoreFramework::DiemVMConfig;
    friend CoreFramework::ParallelExecutionConfig;

    /// Event that signals DiemBFT algorithm to start a new epoch,
    /// with new configuration information. This is also called a
    /// "reconfiguration event"
    struct NewEpochEvent has drop, store {
        epoch: u64,
    }

    /// Holds information about state of reconfiguration
    struct Configuration has key {
        /// Epoch number
        epoch: u64,
        /// Time of last reconfiguration. Only changes on reconfiguration events.
        last_reconfiguration_time: u64,
        /// Event handle for reconfiguration events
        events: event::EventHandle<NewEpochEvent>,
    }

    /// Reconfiguration disabled if this resource occurs under LibraRoot.
    struct DisableReconfiguration has key {}

    /// The `Configuration` resource is in an invalid state
    const ECONFIGURATION: u64 = 0;
    /// A `DiemConfig` resource is in an invalid state
    const EDIEM_CONFIG: u64 = 1;
    /// A `ModifyConfigCapability` is in a different state than was expected
    const EMODIFY_CAPABILITY: u64 = 2;
    /// An invalid block time was encountered.
    const EINVALID_BLOCK_TIME: u64 = 3;
    /// The largest possible u64 value
    const MAX_U64: u64 = 18446744073709551615;

    /// Publishes `Configuration` resource. Can only be invoked by Diem root, and only a single time in Genesis.
    public fun initialize(
        account: &signer,
    ) {
        DiemTimestamp::assert_genesis();
        SystemAddresses::assert_core_resource(account);
        assert!(!exists<Configuration>(@CoreResources), errors::already_published(ECONFIGURATION));
        move_to<Configuration>(
            account,
            Configuration {
                epoch: 0,
                last_reconfiguration_time: 0,
                events: event::new_event_handle<NewEpochEvent>(account),
            }
        );
    }

    /// Private function to temporarily halt reconfiguration.
    /// This function should only be used for offline WriteSet generation purpose and should never be invoked on chain.
    fun disable_reconfiguration(account: &signer) {
        SystemAddresses::assert_core_resource(account);
        assert!(reconfiguration_enabled(), errors::invalid_state(ECONFIGURATION));
        move_to(account, DisableReconfiguration {} )
    }

    /// Private function to resume reconfiguration.
    /// This function should only be used for offline WriteSet generation purpose and should never be invoked on chain.
    fun enable_reconfiguration(account: &signer) acquires DisableReconfiguration {
        SystemAddresses::assert_core_resource(account);

        assert!(!reconfiguration_enabled(), errors::invalid_state(ECONFIGURATION));
        DisableReconfiguration {} = move_from<DisableReconfiguration>(signer::address_of(account));
    }

    fun reconfiguration_enabled(): bool {
        !exists<DisableReconfiguration>(@CoreResources)
    }

    /// Signal validators to start using new configuration. Must be called from friend config modules.
    public(friend) fun reconfigure() acquires Configuration {
        reconfigure_();
    }

    /// Private function to do reconfiguration.  Updates reconfiguration status resource
    /// `Configuration` and emits a `NewEpochEvent`
    fun reconfigure_() acquires Configuration {
        // Do not do anything if genesis has not finished.
        if (DiemTimestamp::is_genesis() || DiemTimestamp::now_microseconds() == 0 || !reconfiguration_enabled()) {
            return ()
        };

        let config_ref = borrow_global_mut<Configuration>(@CoreResources);
        let current_time = DiemTimestamp::now_microseconds();

        // Do not do anything if a reconfiguration event is already emitted within this transaction.
        //
        // This is OK because:
        // - The time changes in every non-empty block
        // - A block automatically ends after a transaction that emits a reconfiguration event, which is guaranteed by
        //   DiemVM spec that all transactions comming after a reconfiguration transaction will be returned as Retry
        //   status.
        // - Each transaction must emit at most one reconfiguration event
        //
        // Thus, this check ensures that a transaction that does multiple "reconfiguration required" actions emits only
        // one reconfiguration event.
        //
        if (current_time == config_ref.last_reconfiguration_time) {
            return
        };

        assert!(current_time > config_ref.last_reconfiguration_time, errors::invalid_state(EINVALID_BLOCK_TIME));
        config_ref.last_reconfiguration_time = current_time;
        config_ref.epoch = config_ref.epoch + 1;

        event::emit_event<NewEpochEvent>(
            &mut config_ref.events,
            NewEpochEvent {
                epoch: config_ref.epoch,
            },
        );
    }

    /// Emit a `NewEpochEvent` event. This function will be invoked by genesis directly to generate the very first
    /// reconfiguration event.
    fun emit_genesis_reconfiguration_event() acquires Configuration {
        assert!(exists<Configuration>(@CoreResources), errors::not_published(ECONFIGURATION));
        let config_ref = borrow_global_mut<Configuration>(@CoreResources);
        assert!(config_ref.epoch == 0 && config_ref.last_reconfiguration_time == 0, errors::invalid_state(ECONFIGURATION));
        config_ref.epoch = 1;

        event::emit_event<NewEpochEvent>(
            &mut config_ref.events,
            NewEpochEvent {
                epoch: config_ref.epoch,
            },
        );
    }
}

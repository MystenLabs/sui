/// This module defines structs and methods to initialize VM configurations,
/// including different costs of running the VM.
module CoreFramework::DiemVMConfig {
    use std::capability::Cap;
    use std::errors;
    use CoreFramework::DiemConfig;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    /// Error with chain marker
    const ECHAIN_MARKER: u64 = 0;
    /// Error with config
    const ECONFIG: u64 = 1;
    /// The provided gas constants were inconsistent.
    const EGAS_CONSTANT_INCONSISTENCY: u64 = 2;

    /// Marker to be stored under @CoreResources during genesis
    struct VMConfigChainMarker<phantom T> has key {}

    /// The struct to hold config data needed to operate the DiemVM.
    struct DiemVMConfig has key {
        /// Cost of running the VM.
        gas_schedule: GasSchedule,
    }

    /// The gas schedule keeps two separate schedules for the gas:
    /// * The instruction_schedule: This holds the gas for each bytecode instruction.
    /// * The native_schedule: This holds the gas for used (per-byte operated over) for each native
    ///   function.
    /// A couple notes:
    /// 1. In the case that an instruction is deleted from the bytecode, that part of the cost schedule
    ///    still needs to remain the same; once a slot in the table is taken by an instruction, that is its
    ///    slot for the rest of time (since that instruction could already exist in a module on-chain).
    /// 2. The initialization of the module will publish the instruction table to the diem root account
    ///    address, and will preload the vector with the gas schedule for instructions. The VM will then
    ///    load this into memory at the startup of each block.
    struct GasSchedule has copy, drop, store {
        instruction_schedule: vector<u8>,
        native_schedule: vector<u8>,
        gas_constants: GasConstants,
    }

    struct GasConstants has copy, drop, store {
        /// The cost per-byte read from global storage.
        global_memory_per_byte_cost: u64,

        /// The cost per-byte written to storage.
        global_memory_per_byte_write_cost: u64,

        /// The flat minimum amount of gas required for any transaction.
        /// Charged at the start of execution.
        min_transaction_gas_units: u64,

        /// Any transaction over this size will be charged an additional amount per byte.
        large_transaction_cutoff: u64,

        /// The units of gas to be charged per byte over the `large_transaction_cutoff` in addition to
        /// `min_transaction_gas_units` for transactions whose size exceeds `large_transaction_cutoff`.
        intrinsic_gas_per_byte: u64,

        /// ~5 microseconds should equal one unit of computational gas. We bound the maximum
        /// computational time of any given transaction at roughly 20 seconds. We want this number and
        /// `MAX_PRICE_PER_GAS_UNIT` to always satisfy the inequality that
        /// MAXIMUM_NUMBER_OF_GAS_UNITS * MAX_PRICE_PER_GAS_UNIT < min(u64::MAX, GasUnits<GasCarrier>::MAX)
        /// NB: The bound is set quite high since custom scripts aren't allowed except from predefined
        /// and vetted senders.
        maximum_number_of_gas_units: u64,

        /// The minimum gas price that a transaction can be submitted with.
        min_price_per_gas_unit: u64,

        /// The maximum gas unit price that a transaction can be submitted with.
        max_price_per_gas_unit: u64,

        max_transaction_size_in_bytes: u64,
        gas_unit_scaling_factor: u64,
        default_account_size: u64,
    }

    /// Initialize the table under the diem root account
    public fun initialize<T>(
        account: &signer,
        instruction_schedule: vector<u8>,
        native_schedule: vector<u8>,
    ) {
        DiemTimestamp::assert_genesis();

        SystemAddresses::assert_core_resource(account);

        assert!(
            !exists<VMConfigChainMarker<T>>(@CoreResources),
            errors::already_published(ECHAIN_MARKER)
        );

        assert!(
            !exists<DiemVMConfig>(@CoreResources),
            errors::already_published(ECONFIG)
        );

        move_to(account, VMConfigChainMarker<T>{});

        let gas_constants = GasConstants {
            global_memory_per_byte_cost: 4,
            global_memory_per_byte_write_cost: 9,
            min_transaction_gas_units: 600,
            large_transaction_cutoff: 600,
            intrinsic_gas_per_byte: 8,
            maximum_number_of_gas_units: 4000000,
            min_price_per_gas_unit: 0,
            max_price_per_gas_unit: 10000,
            max_transaction_size_in_bytes: 4096,
            gas_unit_scaling_factor: 1000,
            default_account_size: 800,
        };

        move_to(
            account,
            DiemVMConfig {
                gas_schedule: GasSchedule {
                    instruction_schedule,
                    native_schedule,
                    gas_constants,
                }
            },
        );
    }

    public fun set_gas_constants<T>(
        global_memory_per_byte_cost: u64,
        global_memory_per_byte_write_cost: u64,
        min_transaction_gas_units: u64,
        large_transaction_cutoff: u64,
        intrinsic_gas_per_byte: u64,
        maximum_number_of_gas_units: u64,
        min_price_per_gas_unit: u64,
        max_price_per_gas_unit: u64,
        max_transaction_size_in_bytes: u64,
        gas_unit_scaling_factor: u64,
        default_account_size: u64,
        _cap: &Cap<T>
    ) acquires DiemVMConfig {
        DiemTimestamp::assert_operating();

        assert!(exists<VMConfigChainMarker<T>>(@CoreResources), errors::not_published(ECHAIN_MARKER));

        assert!(
            min_price_per_gas_unit <= max_price_per_gas_unit,
            errors::invalid_argument(EGAS_CONSTANT_INCONSISTENCY)
        );
        assert!(
            min_transaction_gas_units <= maximum_number_of_gas_units,
            errors::invalid_argument(EGAS_CONSTANT_INCONSISTENCY)
        );

        assert!(exists<DiemVMConfig>(@CoreResources), errors::not_published(ECONFIG));

        let gas_constants = &mut borrow_global_mut<DiemVMConfig>(@CoreResources).gas_schedule.gas_constants;

        gas_constants.global_memory_per_byte_cost       = global_memory_per_byte_cost;
        gas_constants.global_memory_per_byte_write_cost = global_memory_per_byte_write_cost;
        gas_constants.min_transaction_gas_units         = min_transaction_gas_units;
        gas_constants.large_transaction_cutoff          = large_transaction_cutoff;
        gas_constants.intrinsic_gas_per_byte            = intrinsic_gas_per_byte;
        gas_constants.maximum_number_of_gas_units       = maximum_number_of_gas_units;
        gas_constants.min_price_per_gas_unit            = min_price_per_gas_unit;
        gas_constants.max_price_per_gas_unit            = max_price_per_gas_unit;
        gas_constants.max_transaction_size_in_bytes     = max_transaction_size_in_bytes;
        gas_constants.gas_unit_scaling_factor           = gas_unit_scaling_factor;
        gas_constants.default_account_size              = default_account_size;

        DiemConfig::reconfigure();
    }
}

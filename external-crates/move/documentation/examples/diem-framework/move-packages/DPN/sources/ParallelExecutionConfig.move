/// This module defines structs and methods to initialize VM configurations,
/// including different costs of running the VM.
module DiemFramework::ParallelExecutionConfig {
    use DiemFramework::DiemConfig::{Self, DiemConfig};
    use DiemFramework::DiemTimestamp;
    use DiemFramework::Roles;
    use std::option::{Self, Option};

    /// The struct to hold the read/write set analysis result for the whole Diem Framework.
    struct ParallelExecutionConfig has copy, drop, store {
        /// Serialized analysis result for the Diem Framework.
        /// If this payload is not None, DiemVM will use this config to execute transactions in parallel.
        read_write_analysis_result: Option<vector<u8>>
    }

    /// Enable parallel execution functionality of DiemVM by setting the read_write_set analysis result.
    public fun initialize_parallel_execution(
        dr_account: &signer,
    ) {
        // The permission "UpdateVMConfig" is granted to DiemRoot [[H11]][PERMISSION].
        Roles::assert_diem_root(dr_account);
        DiemConfig::publish_new_config(
            dr_account,
            ParallelExecutionConfig {
                read_write_analysis_result: option::none(),
            },
        );
    }

    public fun enable_parallel_execution_with_config(
       dr_account: &signer,
       read_write_inference_result: vector<u8>,
    ) {
        DiemTimestamp::assert_operating();
        Roles::assert_diem_root(dr_account);
        DiemConfig::set(dr_account, ParallelExecutionConfig {
            read_write_analysis_result: option::some(read_write_inference_result),
        });
    }

    public fun disable_parallel_execution(
       dr_account: &signer,
    ) {
        DiemTimestamp::assert_operating();
        Roles::assert_diem_root(dr_account);
        DiemConfig::set(dr_account, ParallelExecutionConfig {
            read_write_analysis_result: option::none(),
        });
    }

    spec initialize_parallel_execution {
        /// Must abort if the signer does not have the DiemRoot role [[H11]][PERMISSION].
        include Roles::AbortsIfNotDiemRoot{account: dr_account};

        include DiemConfig::PublishNewConfigAbortsIf<ParallelExecutionConfig>;
        include DiemConfig::PublishNewConfigEnsures<ParallelExecutionConfig> {
            payload: ParallelExecutionConfig {
                read_write_analysis_result: option::none(),
            }};
    }

    spec enable_parallel_execution_with_config {
        include DiemTimestamp::AbortsIfNotOperating;
        /// No one can update DiemVMConfig except for the Diem Root account [[H11]][PERMISSION].
        include Roles::AbortsIfNotDiemRoot{account: dr_account};
        include DiemConfig::SetAbortsIf<ParallelExecutionConfig>{account: dr_account };
        ensures DiemConfig::spec_is_published<ParallelExecutionConfig>();

        // TODO: How to replace this assertion since we can't invoke Option::some here?
        //        ensures DiemConfig::get<ParallelExecutionConfig>() == ParallelExecutionConfig {
        //            read_write_analysis_result: result,
        //        };

        ensures old(DiemConfig::spec_has_config()) == DiemConfig::spec_has_config();
    }

    spec disable_parallel_execution {
        include DiemTimestamp::AbortsIfNotOperating;
        /// No one can update DiemVMConfig except for the Diem Root account [[H11]][PERMISSION].
        include Roles::AbortsIfNotDiemRoot{account: dr_account};
        include DiemConfig::SetAbortsIf<ParallelExecutionConfig>{account: dr_account };
        ensures DiemConfig::spec_is_published<ParallelExecutionConfig>();
        ensures DiemConfig::get<ParallelExecutionConfig>() == ParallelExecutionConfig {
            read_write_analysis_result: option::none(),
        };
        ensures old(DiemConfig::spec_has_config()) == DiemConfig::spec_has_config();
    }


    spec module { } // Switch documentation context to module level.

    /// # Access Control

    /// The permission "UpdateParallelExecutionConfig" is granted to DiemRoot [[H11]][PERMISSION].
    spec module {
        invariant [suspendable] forall addr: address
            where exists<DiemConfig<ParallelExecutionConfig>>(addr): addr == @DiemRoot;

        invariant update [suspendable] old(DiemConfig::spec_is_published<ParallelExecutionConfig>())
            && DiemConfig::spec_is_published<ParallelExecutionConfig>()
            && old(DiemConfig::get<ParallelExecutionConfig>()) != DiemConfig::get<ParallelExecutionConfig>()
                ==> Roles::spec_signed_by_diem_root_role();
    }

    // TODO: The following is the old style spec, which can removed later.
    /// No one can update DiemVMConfig except for the Diem Root account [[H11]][PERMISSION].
    spec schema DiemVMConfigRemainsSame {
        ensures old(DiemConfig::spec_is_published<ParallelExecutionConfig>()) ==>
            global<DiemConfig<ParallelExecutionConfig>>(@DiemRoot) ==
                old(global<DiemConfig<ParallelExecutionConfig>>(@DiemRoot));
    }
    spec module {
        apply DiemVMConfigRemainsSame to * except enable_parallel_execution_with_config, disable_parallel_execution;
    }
}

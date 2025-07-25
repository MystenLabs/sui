# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

AndroidPlatformInfo = provider(fields = {
    "name": provider_field(typing.Any, default = None),
})

AndroidToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "aapt2": provider_field(typing.Any, default = None),
        "aapt2_filter_resources": provider_field(typing.Any, default = None),
        "aar_builder": provider_field(typing.Any, default = None),
        "adb": provider_field(typing.Any, default = None),
        "aidl": provider_field(typing.Any, default = None),
        "android_jar": provider_field(typing.Any, default = None),
        "android_bootclasspath": provider_field(typing.Any, default = None),
        "apk_builder": provider_field(typing.Any, default = None),
        "apk_module_graph": provider_field(typing.Any, default = None),
        "app_without_resources_stub": provider_field(typing.Any, default = None),
        "bundle_builder": provider_field(typing.Any, default = None),
        "combine_native_library_dirs": provider_field(typing.Any, default = None),
        "compress_libraries": provider_field(typing.Any, default = None),
        "d8_command": provider_field(typing.Any, default = None),
        "exo_resources_rewriter": provider_field(typing.Any, default = None),
        "exopackage_agent_apk": provider_field(typing.Any, default = None),
        "filter_dex_class_names": provider_field(typing.Any, default = None),
        "filter_prebuilt_native_library_dir": provider_field(typing.Any, default = None),
        "installer": provider_field(typing.Any, default = None),
        "jar_splitter_command": provider_field(typing.Any, default = None),
        "multi_dex_command": provider_field(typing.Any, default = None),
        "copy_string_resources": provider_field(typing.Any, default = None),
        "filter_resources": provider_field(typing.Any, default = None),
        "framework_aidl_file": provider_field(typing.Any, default = None),
        "generate_build_config": provider_field(typing.Any, default = None),
        "generate_manifest": provider_field(typing.Any, default = None),
        "instrumentation_test_can_run_locally": provider_field(typing.Any, default = None),
        "instrumentation_test_runner_classpath": provider_field(typing.Any, default = None),
        "instrumentation_test_runner_main_class": provider_field(typing.Any, default = None),
        "manifest_utils": provider_field(typing.Any, default = None),
        "merge_android_resources": provider_field(typing.Any, default = None),
        "merge_android_resource_sources": provider_field(typing.Any, default = None),
        "merge_assets": provider_field(typing.Any, default = None),
        "mergemap_tool": provider_field(typing.Any, default = None),
        "mini_aapt": provider_field(typing.Any, default = None),
        "native_libs_as_assets_metadata": provider_field(typing.Any, default = None),
        "optimized_proguard_config": provider_field(typing.Any, default = None),
        "package_meta_inf_version_files": provider_field(typing.Any, default = None),
        "package_strings_as_assets": provider_field(typing.Any, default = None),
        "prebuilt_aar_resources_have_low_priority": provider_field(typing.Any, default = None),
        "proguard_config": provider_field(typing.Any, default = None),
        "proguard_jar": provider_field(typing.Any, default = None),
        "proguard_max_heap_size": provider_field(typing.Any, default = None),
        "r_dot_java_weight_factor": provider_field(typing.Any, default = None),
        "replace_application_id_placeholders": provider_field(typing.Any, default = None),
        "secondary_dex_compression_command": provider_field(typing.Any, default = None),
        "secondary_dex_weight_limit": provider_field(typing.Any, default = None),
        "set_application_id_to_specified_package": provider_field(typing.Any, default = None),
        "should_run_sanity_check_for_placeholders": provider_field(typing.Any, default = None),
        "unpack_aar": provider_field(typing.Any, default = None),
        "zipalign": provider_field(typing.Any, default = None),
    },
)

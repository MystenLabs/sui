# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

AbiGenerationMode = enum("class", "none", "source", "source_only")

DepFiles = enum("none", "per_class", "per_jar")

JavacProtocol = enum("classic", "javacd")

JavaPlatformInfo = provider(
    doc = "Java platform info",
    fields = {
        "name": provider_field(typing.Any, default = None),
    },
)

JavaToolchainInfo = provider(
    # @unsorted-dict-items
    doc = "Java toolchain info",
    fields = {
        "abi_generation_mode": provider_field(typing.Any, default = None),
        "bootclasspath_7": provider_field(typing.Any, default = None),
        "bootclasspath_8": provider_field(typing.Any, default = None),
        "class_abi_generator": provider_field(typing.Any, default = None),
        "class_loader_bootstrapper": provider_field(typing.Any, default = None),
        "compile_and_package": provider_field(typing.Any, default = None),
        "dep_files": provider_field(typing.Any, default = None),
        "fat_jar": provider_field(typing.Any, default = None),
        "fat_jar_main_class_lib": provider_field(typing.Any, default = None),
        "gen_class_to_source_map": provider_field(typing.Any, default = None),
        "gen_class_to_source_map_debuginfo": provider_field(typing.Any, default = None),  # optional
        "is_bootstrap_toolchain": provider_field(typing.Any, default = None),
        "jar": provider_field(typing.Any, default = None),
        "jar_builder": provider_field(typing.Any, default = None),
        "java": provider_field(typing.Any, default = None),
        "java_for_tests": provider_field(typing.Any, default = None),
        "javac": provider_field(typing.Any, default = None),
        "javac_protocol": provider_field(typing.Any, default = None),
        "javacd_debug_port": provider_field(typing.Any, default = None),
        "javacd_debug_target": provider_field(typing.Any, default = None),
        "javacd_jvm_args": provider_field(typing.Any, default = None),
        "javacd_jvm_args_target": provider_field(typing.Any, default = None),
        "javacd_main_class": provider_field(typing.Any, default = None),
        "javacd_worker": provider_field(typing.Any, default = None),
        "nullsafe": provider_field(typing.Any, default = None),
        "nullsafe_extra_args": provider_field(typing.Any, default = None),
        "nullsafe_signatures": provider_field(typing.Any, default = None),
        "source_level": provider_field(typing.Any, default = None),
        "src_root_elements": provider_field(typing.Any, default = None),
        "src_root_prefixes": provider_field(typing.Any, default = None),
        "target_level": provider_field(typing.Any, default = None),
        "zip_scrubber": provider_field(typing.Any, default = None),
    },
)

JavaTestToolchainInfo = provider(
    # @unsorted-dict-items
    doc = "Java test toolchain info",
    fields = {
        "java_custom_class_loader_class": provider_field(typing.Any, default = None),
        "java_custom_class_loader_library_jar": provider_field(typing.Any, default = None),
        "java_custom_class_loader_vm_args": provider_field(typing.Any, default = None),
        "junit5_test_runner_main_class_args": provider_field(typing.Any, default = None),
        "junit_test_runner_main_class_args": provider_field(typing.Any, default = None),
        "list_class_names": provider_field(typing.Any, default = None),
        "merge_class_to_source_maps": provider_field(typing.Any, default = None),
        "test_runner_library_jar": provider_field(typing.Any, default = None),
        "testng_test_runner_main_class_args": provider_field(typing.Any, default = None),
        "use_java_custom_class_loader": provider_field(typing.Any, default = None),
    },
)

# prebuilt_jar needs so little of the Java toolchain that it's worth
# giving it its own to reduce the occurrence of cycles as we add
# more Java- and Kotlin-built tools to the Java and Kotlin toolchains
PrebuiltJarToolchainInfo = provider(
    doc = "prebuilt_jar toolchain info",
    fields = {
        "class_abi_generator": provider_field(typing.Any, default = None),
        "is_bootstrap_toolchain": provider_field(typing.Any, default = None),
    },
)

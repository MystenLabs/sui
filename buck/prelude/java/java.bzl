# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//android:build_only_native_code.bzl", "is_build_only_native_code")
load("@prelude//android:configuration.bzl", "is_building_android_binary_attr")
load("@prelude//android:min_sdk_version.bzl", "get_min_sdk_version_constraint_value_name", "get_min_sdk_version_range")
load("@prelude//java/plugins:java_annotation_processor.bzl", "java_annotation_processor_impl")
load("@prelude//java/plugins:java_plugin.bzl", "java_plugin_impl")
load("@prelude//decls/common.bzl", "buck")
load("@prelude//decls/toolchains_common.bzl", "toolchains_common")
load("@prelude//genrule.bzl", "genrule_attributes")
load(":jar_genrule.bzl", "jar_genrule_impl")
load(":java_binary.bzl", "java_binary_impl")
load(":java_library.bzl", "java_library_impl")
load(":java_test.bzl", "java_test_impl")
load(":keystore.bzl", "keystore_impl")
load(":prebuilt_jar.bzl", "prebuilt_jar_impl")

AbiGenerationMode = ["class", "source", "source_only", "none"]

def dex_min_sdk_version():
    min_sdk_version_dict = {"DEFAULT": None}
    for min_sdk in get_min_sdk_version_range():
        constraint = "prelude//android/constraints:{}".format(get_min_sdk_version_constraint_value_name(min_sdk))
        min_sdk_version_dict[constraint] = min_sdk

    return select(min_sdk_version_dict)

implemented_rules = {
    "jar_genrule": jar_genrule_impl,
    "java_annotation_processor": java_annotation_processor_impl,
    "java_binary": java_binary_impl,
    "java_library": java_library_impl,
    "java_plugin": java_plugin_impl,
    "java_test": java_test_impl,
    "keystore": keystore_impl,
    "prebuilt_jar": prebuilt_jar_impl,
}

extra_attributes = {
    "jar_genrule": genrule_attributes() | {
        "_java_toolchain": toolchains_common.java(),
    },
    "java_annotation_processor": {
        "_build_only_native_code": attrs.default_only(attrs.bool(default = is_build_only_native_code())),
    },
    "java_binary": {
        "java_args_for_run_info": attrs.list(attrs.string(), default = []),
        "meta_inf_directory": attrs.option(attrs.source(allow_directory = True), default = None),
        "_build_only_native_code": attrs.default_only(attrs.bool(default = is_build_only_native_code())),
        "_exec_os_type": buck.exec_os_type_arg(),
        "_is_building_android_binary": is_building_android_binary_attr(),
        "_java_toolchain": toolchains_common.java(),
    },
    "java_library": {
        "abi_generation_mode": attrs.option(attrs.enum(AbiGenerationMode), default = None),
        "javac": attrs.option(attrs.one_of(attrs.dep(), attrs.source()), default = None),
        "resources_root": attrs.option(attrs.string(), default = None),
        "_build_only_native_code": attrs.default_only(attrs.bool(default = is_build_only_native_code())),
        "_dex_min_sdk_version": attrs.option(attrs.int(), default = dex_min_sdk_version()),
        "_dex_toolchain": toolchains_common.dex(),
        "_exec_os_type": buck.exec_os_type_arg(),
        "_is_building_android_binary": is_building_android_binary_attr(),
        "_java_toolchain": toolchains_common.java(),
    },
    "java_plugin": {
        "_build_only_native_code": attrs.default_only(attrs.bool(default = is_build_only_native_code())),
    },
    "java_test": {
        "abi_generation_mode": attrs.option(attrs.enum(AbiGenerationMode), default = None),
        "javac": attrs.option(attrs.one_of(attrs.dep(), attrs.source()), default = None),
        "resources_root": attrs.option(attrs.string(), default = None),
        "unbundled_resources_root": attrs.option(attrs.source(allow_directory = True), default = None),
        "_build_only_native_code": attrs.default_only(attrs.bool(default = is_build_only_native_code())),
        "_exec_os_type": buck.exec_os_type_arg(),
        "_is_building_android_binary": attrs.default_only(attrs.bool(default = False)),
        "_java_test_toolchain": toolchains_common.java_test(),
        "_java_toolchain": toolchains_common.java(),
    },
    "java_test_runner": {
        "abi_generation_mode": attrs.option(attrs.enum(AbiGenerationMode), default = None),
        "resources_root": attrs.option(attrs.string(), default = None),
    },
    "prebuilt_jar": {
        "generate_abi": attrs.bool(default = True),
        # Prebuilt jars are quick to build, and often contain third-party code, which in turn is
        # often a source of annotations and constants. To ease migration to ABI generation from
        # source without deps, we have them present during ABI gen by default.
        "required_for_source_only_abi": attrs.bool(default = True),
        "_build_only_native_code": attrs.default_only(attrs.bool(default = is_build_only_native_code())),
        "_dex_min_sdk_version": attrs.option(attrs.int(), default = dex_min_sdk_version()),
        "_dex_toolchain": toolchains_common.dex(),
        "_exec_os_type": buck.exec_os_type_arg(),
        "_prebuilt_jar_toolchain": toolchains_common.prebuilt_jar(),
    },
}

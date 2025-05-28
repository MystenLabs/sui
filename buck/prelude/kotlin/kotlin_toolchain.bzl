# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

KotlincProtocol = enum("classic", "kotlincd")

KotlinToolchainInfo = provider(
    # @unsorted-dict-items
    doc = "Kotlin toolchain info",
    fields = {
        "annotation_processing_jar": provider_field(typing.Any, default = None),
        "class_loader_bootstrapper": provider_field(typing.Any, default = None),
        "compilation_tracer_plugin": provider_field(typing.Any, default = None),
        "compile_kotlin": provider_field(typing.Any, default = None),
        "dep_files": provider_field(typing.Any, default = None),
        "jvm_abi_gen_plugin": provider_field(typing.Any, default = None),
        "kapt_base64_encoder": provider_field(typing.Any, default = None),
        "kosabi_applicability_plugin": provider_field(typing.Any, default = None),
        "kosabi_jvm_abi_gen_plugin": provider_field(typing.Any, default = None),
        "kosabi_stubs_gen_plugin": provider_field(typing.Any, default = None),
        "kosabi_supported_ksp_providers": provider_field(typing.Any, default = None),
        "kotlin_home_libraries": provider_field(typing.Any, default = None),
        "kotlin_stdlib": provider_field(typing.Any, default = None),
        "kotlinc": provider_field(typing.Any, default = None),
        "kotlinc_classpath": provider_field(typing.Any, default = None),
        "kotlinc_protocol": provider_field(typing.Any, default = None),
        "kotlincd_debug_port": provider_field(typing.Any, default = None),
        "kotlincd_debug_target": provider_field(typing.Any, default = None),
        "kotlincd_jvm_args": provider_field(typing.Any, default = None),
        "kotlincd_jvm_args_target": provider_field(typing.Any, default = None),
        "kotlincd_main_class": provider_field(typing.Any, default = None),
        "kotlincd_worker": provider_field(typing.Any, default = None),
        "qpld_dotslash": provider_field(typing.Any, default = None),
        "should_use_compilation_tracer": provider_field(typing.Any, default = None),
        "track_class_usage_plugin": provider_field(typing.Any, default = None),
    },
)

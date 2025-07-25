# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//utils:arglike.bzl", "ArgLike")  # @unused Used as a type
load(":apple_bundle_utility.bzl", "get_default_binary_dep")
load(":apple_code_signing_types.bzl", "AppleEntitlementsInfo", "CodeSignType")
load(":apple_sdk_metadata.bzl", "IPhoneSimulatorSdkMetadata", "MacOSXCatalystSdkMetadata")
load(":apple_toolchain_types.bzl", "AppleToolchainInfo")

def get_entitlements_codesign_args(ctx: AnalysisContext, codesign_type: CodeSignType) -> list[ArgLike]:
    include_entitlements = _should_include_entitlements(ctx, codesign_type)
    maybe_entitlements = _entitlements_file(ctx) if include_entitlements else None
    entitlements_args = ["--entitlements", maybe_entitlements] if maybe_entitlements else []
    return entitlements_args

def _should_include_entitlements(ctx: AnalysisContext, codesign_type: CodeSignType) -> bool:
    if codesign_type.value == "distribution":
        return True

    if codesign_type.value == "adhoc":
        # The config-based override value takes priority over target value
        if ctx.attrs._use_entitlements_when_adhoc_code_signing != None:
            return ctx.attrs._use_entitlements_when_adhoc_code_signing
        return ctx.attrs.use_entitlements_when_adhoc_code_signing

    return False

def _entitlements_file(ctx: AnalysisContext) -> [Artifact, None]:
    if hasattr(ctx.attrs, "entitlements_file"):
        # Bundling `apple_test` which doesn't have a binary to provide the entitlements, so they are provided via `entitlements_file` attribute directly.
        return ctx.attrs.entitlements_file

    if not ctx.attrs.binary:
        return None

    # The `binary` attribute can be either an apple_binary or a dynamic library from apple_library
    binary_entitlement_info = get_default_binary_dep(ctx.attrs.binary)[AppleEntitlementsInfo]
    if binary_entitlement_info and binary_entitlement_info.entitlements_file:
        return binary_entitlement_info.entitlements_file

    return ctx.attrs._codesign_entitlements

_SDK_NAMES_NEED_ENTITLEMENTS_IN_BINARY = [
    IPhoneSimulatorSdkMetadata.name,
    MacOSXCatalystSdkMetadata.name,
]

def _needs_entitlements_in_binary(ctx: AnalysisContext) -> bool:
    apple_toolchain_info = ctx.attrs._apple_toolchain[AppleToolchainInfo]
    return apple_toolchain_info.sdk_name in _SDK_NAMES_NEED_ENTITLEMENTS_IN_BINARY

def entitlements_link_flags(ctx: AnalysisContext) -> list[typing.Any]:
    return [
        "-Xlinker",
        "-sectcreate",
        "-Xlinker",
        "__TEXT",
        "-Xlinker",
        "__entitlements",
        "-Xlinker",
        ctx.attrs.entitlements_file,
    ] if (ctx.attrs.entitlements_file and _needs_entitlements_in_binary(ctx)) else []

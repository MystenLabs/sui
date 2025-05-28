# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:paths.bzl", "paths")
load("@prelude//utils:utils.bzl", "expect")
load(":apple_bundle_destination.bzl", "AppleBundleDestination", "bundle_relative_path_for_destination")
load(":apple_bundle_utility.bzl", "get_extension_attr", "get_product_name")
load(":apple_code_signing_types.bzl", "CodeSignType")
load(":apple_entitlements.bzl", "get_entitlements_codesign_args")
load(":apple_sdk.bzl", "get_apple_sdk_name")
load(":apple_sdk_metadata.bzl", "get_apple_sdk_metadata_for_sdk_name")
load(":apple_swift_stdlib.bzl", "should_copy_swift_stdlib")
load(":apple_toolchain_types.bzl", "AppleToolchainInfo", "AppleToolsInfo")

# Defines where and what should be copied into
AppleBundlePart = record(
    # A file or directory which content should be copied
    source = field(Artifact),
    # Where the source should be copied, the actual destination directory
    # inside bundle depends on target platform
    destination = AppleBundleDestination,
    # New file name if it should be renamed before copying.
    # Empty string value is applicable only when `source` is a directory,
    # in such case only content of the directory will be copied, as opposed to the directory itself.
    # When value is `None`, directory or file will be copied as it is, without renaming.
    new_name = field([str, None], None),
    # Marks parts which should be code signed separately from the whole bundle.
    codesign_on_copy = field(bool, False),
)

SwiftStdlibArguments = record(
    primary_binary_rel_path = field(str),
)

def bundle_output(ctx: AnalysisContext) -> Artifact:
    bundle_dir_name = get_bundle_dir_name(ctx)
    output = ctx.actions.declare_output(bundle_dir_name)
    return output

def assemble_bundle(
        ctx: AnalysisContext,
        bundle: Artifact,
        parts: list[AppleBundlePart],
        info_plist_part: [AppleBundlePart, None],
        swift_stdlib_args: [SwiftStdlibArguments, None],
        extra_hidden: list[Artifact] = [],
        skip_adhoc_signing: bool = False) -> dict[str, list[Provider]]:
    """
    Returns extra subtargets related to bundling.
    """
    all_parts = parts + [info_plist_part] if info_plist_part else []
    spec_file = _bundle_spec_json(ctx, all_parts)

    tools = ctx.attrs._apple_tools[AppleToolsInfo]
    tool = tools.assemble_bundle

    codesign_args = []
    codesign_type = _detect_codesign_type(ctx, skip_adhoc_signing)

    codesign_tool = ctx.attrs._apple_toolchain[AppleToolchainInfo].codesign
    if ctx.attrs._dry_run_code_signing:
        codesign_configuration_args = ["--codesign-configuration", "dry-run"]
        codesign_tool = tools.dry_codesign_tool
    elif ctx.attrs._fast_adhoc_signing_enabled:
        codesign_configuration_args = ["--codesign-configuration", "fast-adhoc"]
    else:
        codesign_configuration_args = []

    codesign_required = codesign_type.value in ["distribution", "adhoc"]
    swift_support_required = swift_stdlib_args and (not ctx.attrs.skip_copying_swift_stdlib) and should_copy_swift_stdlib(bundle.extension)

    sdk_name = get_apple_sdk_name(ctx)
    if codesign_required or swift_support_required:
        platform_args = ["--platform", sdk_name]
    else:
        platform_args = []

    if swift_support_required:
        swift_args = [
            "--binary-destination",
            swift_stdlib_args.primary_binary_rel_path,
            "--frameworks-destination",
            bundle_relative_path_for_destination(AppleBundleDestination("frameworks"), sdk_name, ctx.attrs.extension),
            "--plugins-destination",
            bundle_relative_path_for_destination(AppleBundleDestination("plugins"), sdk_name, ctx.attrs.extension),
            "--appclips-destination",
            bundle_relative_path_for_destination(AppleBundleDestination("appclips"), sdk_name, ctx.attrs.extension),
            "--swift-stdlib-command",
            cmd_args(ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info.swift_stdlib_tool, delimiter = " ", quote = "shell"),
            "--sdk-root",
            ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info.sdk_path,
        ]
    else:
        swift_args = []

    if codesign_required:
        codesign_args = [
            "--codesign",
            "--codesign-tool",
            codesign_tool,
        ]

        if codesign_type.value != "adhoc":
            provisioning_profiles = ctx.attrs._provisioning_profiles[DefaultInfo]
            expect(
                len(provisioning_profiles.default_outputs) == 1,
                "expected exactly one default output from provisioning profile",
            )
            provisioning_profiles_args = ["--profiles-dir"] + provisioning_profiles.default_outputs
            codesign_args.extend(provisioning_profiles_args)

            identities_command = ctx.attrs._apple_toolchain[AppleToolchainInfo].codesign_identities_command
            identities_command_args = ["--codesign-identities-command", cmd_args(identities_command)] if identities_command else []
            codesign_args.extend(identities_command_args)
        else:
            codesign_args.append("--ad-hoc")
            if ctx.attrs.codesign_identity:
                codesign_args.extend(["--ad-hoc-codesign-identity", ctx.attrs.codesign_identity])

        codesign_args += get_entitlements_codesign_args(ctx, codesign_type)

        info_plist_args = [
            "--info-plist-source",
            info_plist_part.source,
            "--info-plist-destination",
            get_apple_bundle_part_relative_destination_path(ctx, info_plist_part),
        ] if info_plist_part else []
        codesign_args.extend(info_plist_args)
    elif codesign_type.value == "skip":
        pass
    else:
        fail("Code sign type `{}` not supported".format(codesign_type))

    command = cmd_args([
        tool,
        "--output",
        bundle.as_output(),
        "--spec",
        spec_file,
    ] + codesign_args + platform_args + swift_args)
    command.hidden([part.source for part in all_parts])
    run_incremental_args = {}
    incremental_state = ctx.actions.declare_output("incremental_state.json").as_output()

    # Fallback to value from buckconfig
    incremental_bundling_enabled = ctx.attrs.incremental_bundling_enabled or ctx.attrs._incremental_bundling_enabled

    if incremental_bundling_enabled:
        command.add("--incremental-state", incremental_state)
        run_incremental_args = {
            "metadata_env_var": "ACTION_METADATA",
            "metadata_path": "action_metadata.json",
            "no_outputs_cleanup": True,
        }
        category = "apple_assemble_bundle_incremental"
    else:
        # overwrite file with incremental state so if previous and next builds are incremental
        # (as opposed to the current non-incremental one), next one won't assume there is a
        # valid incremental state.
        command.hidden(ctx.actions.write_json(incremental_state, {}))
        category = "apple_assemble_bundle"

    if ctx.attrs._profile_bundling_enabled:
        profile_output = ctx.actions.declare_output("bundling_profile.txt").as_output()
        command.add("--profile-output", profile_output)

    subtargets = {}
    if ctx.attrs._bundling_log_file_enabled:
        bundling_log_output = ctx.actions.declare_output("bundling_log.txt")
        command.add("--log-file", bundling_log_output.as_output())
        if ctx.attrs._bundling_log_file_level:
            command.add("--log-level-file", ctx.attrs._bundling_log_file_level)
        subtargets["bundling-log"] = [DefaultInfo(default_output = bundling_log_output)]

    if ctx.attrs._bundling_path_conflicts_check_enabled:
        command.add("--check-conflicts")

    command.add(codesign_configuration_args)

    # Ensures any genrule deps get built, such targets are used for validation
    command.hidden(extra_hidden)

    env = {}
    cache_buster = ctx.attrs._bundling_cache_buster
    if cache_buster:
        env["BUCK2_BUNDLING_CACHE_BUSTER"] = cache_buster

    force_local_bundling = codesign_type.value != "skip"
    ctx.actions.run(
        command,
        local_only = force_local_bundling,
        prefer_local = not force_local_bundling,
        category = category,
        env = env,
        **run_incremental_args
    )
    return subtargets

def get_bundle_dir_name(ctx: AnalysisContext) -> str:
    return paths.replace_extension(get_product_name(ctx), "." + get_extension_attr(ctx))

def get_apple_bundle_part_relative_destination_path(ctx: AnalysisContext, part: AppleBundlePart) -> str:
    bundle_relative_path = bundle_relative_path_for_destination(part.destination, get_apple_sdk_name(ctx), ctx.attrs.extension)
    destination_file_or_directory_name = part.new_name if part.new_name != None else paths.basename(part.source.short_path)
    return paths.join(bundle_relative_path, destination_file_or_directory_name)

# Returns JSON to be passed into bundle assembling tool. It should contain a dictionary which maps bundle relative destination paths to source paths."
def _bundle_spec_json(ctx: AnalysisContext, parts: list[AppleBundlePart]) -> Artifact:
    specs = []

    for part in parts:
        part_spec = {
            "dst": get_apple_bundle_part_relative_destination_path(ctx, part),
            "src": part.source,
        }
        if part.codesign_on_copy:
            part_spec["codesign_on_copy"] = True
        specs.append(part_spec)

    return ctx.actions.write_json("bundle_spec.json", specs)

def _detect_codesign_type(ctx: AnalysisContext, skip_adhoc_signing: bool) -> CodeSignType:
    def compute_codesign_type():
        if ctx.attrs.extension not in ["app", "appex", "xctest"]:
            # Only code sign application bundles, extensions and test bundles
            return CodeSignType("skip")

        if ctx.attrs._codesign_type:
            return CodeSignType(ctx.attrs._codesign_type)
        sdk_name = get_apple_sdk_name(ctx)
        is_ad_hoc_sufficient = get_apple_sdk_metadata_for_sdk_name(sdk_name).is_ad_hoc_code_sign_sufficient
        return CodeSignType("adhoc" if is_ad_hoc_sufficient else "distribution")

    codesign_type = compute_codesign_type()
    if skip_adhoc_signing and codesign_type.value == "adhoc":
        codesign_type = CodeSignType("skip")

    return codesign_type

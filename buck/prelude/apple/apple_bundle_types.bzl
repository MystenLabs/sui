# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(":debug.bzl", "AppleDebuggableInfo")

AppleBundleType = enum(
    "default",
    # Bundle was built for watchOS Apple platform
    "watchapp",
    # Bundle represents an App Clip to be embedded
    "appclip",
)

# Provider flagging that result of the rule contains Apple bundle.
# It might be copied into main bundle to appropriate place if rule
# with this provider is a dependency of `apple_bundle`.
AppleBundleInfo = provider(
    # @unsorted-dict-items
    fields = {
        # Result bundle
        "bundle": provider_field(Artifact),
        "bundle_type": provider_field(AppleBundleType),
        # The name of the executable within the bundle.
        "binary_name": provider_field([str, None], default = None),
        # If the bundle contains a Watch Extension executable, we have to update the packaging.
        # Similar to `is_watchos`, this might be omitted for certain types of bundles which don't depend on it.
        "contains_watchapp": provider_field([bool, None]),
        # By default, non-framework, non-appex binaries copy Swift libraries into the final
        # binary. This is the opt-out for that.
        "skip_copying_swift_stdlib": provider_field([bool, None]),
    },
)

# Provider which helps to propagate minimum deployment version up the target graph.
AppleMinDeploymentVersionInfo = provider(fields = {
    "version": provider_field([str, None]),
})

AppleBundleResourceInfo = provider(fields = {
    "resource_output": provider_field(typing.Any, default = None),  # AppleBundleResourcePartListOutput
})

AppleBundleLinkerMapInfo = provider(fields = {
    "linker_maps": provider_field(list[Artifact]),
})

# Providers used to merge extra linker outputs as a top level output
# of an application bundle.
AppleBinaryExtraOutputsInfo = provider(fields = {
    "default_output": provider_field(Artifact),
    "extra_outputs": provider_field(dict[str, list[Artifact]]),
    "name": provider_field(str),
})

AppleBundleExtraOutputsInfo = provider(fields = {
    "extra_outputs": provider_field(list[AppleBinaryExtraOutputsInfo]),
})

AppleBundleBinaryOutput = record(
    binary = field(Artifact),
    debuggable_info = field([AppleDebuggableInfo, None], None),
    # In the case of watchkit, the `ctx.attrs.binary`'s not set, and we need to create a stub binary.
    is_watchkit_stub_binary = field(bool, False),
)

AppleBundleTypeDefault = AppleBundleType("default")
AppleBundleTypeWatchApp = AppleBundleType("watchapp")
AppleBundleTypeAppClip = AppleBundleType("appclip")

# Represents the user-visible type which is distinct from the internal one (`AppleBundleType`)
AppleBundleTypeAttributeType = enum(
    "appclip",
)

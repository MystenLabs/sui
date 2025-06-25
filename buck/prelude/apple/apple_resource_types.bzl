# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//utils:arglike.bzl", "ArgLike")  # @unused Used as a type

# Represents the values for the `destination` field of `apple_resource`
AppleResourceDestination = enum(
    "executables",
    "frameworks",
    "loginitems",
    "plugins",
    "resources",
    "xpcservices",
)

# Defines _where_ resources need to be placed in an `apple_bundle`
AppleResourceSpec = record(
    files = field(list[[Artifact, Dependency]], []),
    dirs = field(list[Artifact], []),
    content_dirs = field(list[Artifact], []),
    destination = AppleResourceDestination,
    variant_files = field(list[Artifact], []),
    # Map from locale to list of files for that locale, e.g.
    # `{ "ru.lproj" : ["Localizable.strings"] }`
    named_variant_files = field(dict[str, list[Artifact]], {}),
    codesign_files_on_copy = field(bool, False),
)

# Used when invoking `ibtool`, `actool` and `momc`
AppleResourceProcessingOptions = record(
    prefer_local = field(bool, False),
    allow_cache_upload = field(bool, False),
)

CxxResourceSpec = record(
    resources = field(dict[str, (Artifact, list[ArgLike])], {}),
)

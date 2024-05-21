# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Splitting the resource compilation into another rule means we can have
# different exec platforms for the resource compilation and for the rest
# of the bundling process. This allows us to send resource compilations
# directly to RE.
#
# +-------------------------------------------------+
# |                 apple_bundle()                  |
# |           Exec Platform: macOS/Linux            |
# |   +--------+  +--------+ +------------------+   |
# +---+ binary +--+  deps  +-+ _resource_bundle +---+
#     +--------+  +--------+ +------------------+
#          |           |               |
#          |           |               |
#          |           |               +---------------+
#          |           |                               |
#          |           |                               |
#          |           |                               v
#          |           |              +---------------------------------+
#          |           +-----+        |     apple_resource_bundle()     |
#          |                 |        |    Exec Platform: macOS-only    |
#          |                 |        |     +--------+  +--------+      |
#          |                 |        +-----+ binary +--+  deps  +------+
#          |                 |              +--------+  +--------+
#          |                 |                   |           |
#          |                 |                   |           |
#          |                 v                   |           |
#          |       +-------------------+         |           |
#          |       |   Dependencies    |<--------+-----------+
#          |       +-------------------+         |
#          |       +-------------------+         |
#          +------>|      Binary       |<--------+
#                  +-------------------+

load("@prelude//apple:apple_bundle_attrs.bzl", "get_apple_info_plist_build_system_identification_attrs")

_RESOURCE_BUNDLE_FIELDS = [
    "asset_catalogs_compilation_options",
    "binary",
    "default_target_platform",
    "deps",
    "extension",
    "ibtool_flags",
    "ibtool_module_flag",
    "info_plist",
    "info_plist_substitutions",
    "product_name",
    "resource_group",
    "resource_group_map",
    "within_view",
    "visibility",
] + get_apple_info_plist_build_system_identification_attrs().keys()

def _is_resources_toolchain_enabled() -> bool:
    is_arvr_query_mode = read_root_config("fb", "arvr_query_mode") in ("True", "true")
    is_xplat_query_mode = read_root_config("mode", "is_xplat_mode_query") in ("True", "true")
    if is_arvr_query_mode or is_xplat_query_mode:
        # Avoid returning buck2-only targets
        return False

    return (read_root_config("apple", "resources_toolchain_enabled", "true").lower() == "true")

def make_resource_bundle_rule(apple_resource_bundle_rule, **kwargs) -> [None, str]:
    # The `apple_resource_bundle()` target will _always_ be Xcode-based, so resources can always be used
    # from there. `resources_toolchain_enabled` exists only as a killswitch (or for testing/debugging purposes).
    # By default, we consistently get all resources from `apple_resource_bundle()` target across all OSes and
    # toolchains.
    resources_toolchain_enabled = _is_resources_toolchain_enabled()
    if not resources_toolchain_enabled:
        return None

    resource_bundle_name = kwargs["name"] + "__ResourceBundle_Private"
    resource_bundle_kwargs = {
        "labels": ["generated"],
        "_bundle_target_name": kwargs["name"],
        "_compile_resources_locally_override": kwargs["_compile_resources_locally_override"],
    }
    for field_name in _RESOURCE_BUNDLE_FIELDS:
        resource_bundle_kwargs[field_name] = kwargs.get(field_name)

    # TODO(T125269558): Remove usage of apple_resource_bundle() once we have exec groups.
    apple_resource_bundle_rule(
        name = resource_bundle_name,
        **resource_bundle_kwargs
    )

    return ":{}".format(resource_bundle_name)

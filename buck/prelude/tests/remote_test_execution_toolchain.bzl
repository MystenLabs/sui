# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

RemoteTestExecutionToolchainInfo = provider(
    fields = [
        # The profile to use by default.
        "default_profile",
        # A dictionary of string names to pre-registered profiles.  Rules can
        # use the profile name to references these.
        "profiles",
    ],
)

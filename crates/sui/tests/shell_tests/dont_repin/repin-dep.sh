# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This should fail - the manifest has a broken dep, and although the lockfile
# has it pinned to the correct location, we've edited a dependency's manifest
# so it should cause repinning
echo "# comment" >> locked_dep_path/Move.toml
sui move build

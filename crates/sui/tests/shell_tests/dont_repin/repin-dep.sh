# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This should fail - the manifest has a broken dep, and although the lockfile
# has it pinned to the correct location, we've edited a dependency's manifest
# so it should cause repinning
echo '[dependencies]' >> locked_dep_path/Move.toml
echo 'another_dep = { local = "../another_dep" }' >> locked_dep_path/Move.toml
sui move --client.config $CONFIG build

# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This should fail - the manifest has a broken dep, and although the lockfile
# has it pinned to the correct location, we've edited the manifest so it should cause repinning
echo "# comment" >> Move.toml
sui move build

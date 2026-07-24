# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Run `cache-package` against an env that is neither declared in the dep's
# manifest nor among the flavor's default envs; should fail.
sui move --client.config $CONFIG cache-package unknown_env abcd1234 "{ local="\"$(pwd -W 2>/dev/null || pwd)/a\"" }"

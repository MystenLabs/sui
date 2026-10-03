# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Run `cache-package` with a valid env name but the WRONG chain id; should fail
# (the env name and id passed by the caller must agree with each other and with
# the publication recorded for that env).
sui move --client.config $CONFIG cache-package custom_env wrongid12 "{ local="\"$(pwd -W 2>/dev/null || pwd)/a\"" }"

# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Should succeed with linting disabled (but stats should be summarized)
sui move --client.config $CONFIG test -p example --silence-warnings

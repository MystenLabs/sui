# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# A recognized lint code resolves successfully.
sui move --client.config $CONFIG explain WSL02001

# An unrecognized code is rejected.
sui move --client.config $CONFIG explain EC99999

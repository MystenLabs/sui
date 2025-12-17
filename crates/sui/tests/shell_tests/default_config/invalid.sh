# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If the config file is a directory, we fail nicely
mkdir client.yaml
sui move --client.config ./client.yaml new example

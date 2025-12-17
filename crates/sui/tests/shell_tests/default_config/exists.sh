# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If the client config file already exists, it doesn't change
cp client-exists.yaml before.yaml
sui move --client.config ./client-exists.yaml -y new example

echo "diffing before/after; there should be no change"
diff client-exists.yaml before.yaml

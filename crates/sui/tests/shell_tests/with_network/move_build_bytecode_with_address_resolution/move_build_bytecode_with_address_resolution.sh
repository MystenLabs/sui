# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

sui client --client.config $CONFIG \
  publish simple --verify-deps \
  --json | jq '.effects.status'

sui move --client.config $CONFIG \
  build --path depends_on_simple

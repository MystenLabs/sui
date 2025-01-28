# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# fixing issue https://github.com/MystenLabs/sui/issues/6546

sui client --client.config $CONFIG \
  publish simple \
  --json | jq '.effects.status'

sui move --client.config $CONFIG \
  build --path depends_on_simple

# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Multiple args (flag + path) are forwarded verbatim, no `--` separator
# required.

chmod +x stubs/prettier-move
export PATH="$PWD/stubs:$(dirname "$(command -v sui)")"

sui move --client.config $CONFIG format -w sources/foo.move

# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# `prettier-move` missing, `npm` present (stubbed), stdin is not a TTY (the
# harness pipes stdin from cargo nextest). `sui move format` must skip the
# interactive prompt and bail with the manual-install hint. The stub `npm`
# only handles `--version`; if the install path is reached by mistake, the
# stub fails loudly.

chmod +x stubs/npm
export PATH="$PWD/stubs:$(dirname "$(command -v sui)")"

sui move --client.config $CONFIG format -c

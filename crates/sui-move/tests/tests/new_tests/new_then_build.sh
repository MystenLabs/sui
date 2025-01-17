# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that sui-move new followed by sui-move build succeeds

sui-move new example
cd example && sui-move build 2>&1 | awk '
  # TODO [DVX-678]: sui-move build is non-deterministic, so this is ugly.
  # We snip out everything between "UPDATING" and "INCLUDING"
  BEGIN { snip = 0 }
  /GIT DEPENDENCY/ { snip = 1; print "  ... snipped git commands ..." }
  /INCLUDING/ { snip = 0 }
  { if (snip == 0) print $0 }
'

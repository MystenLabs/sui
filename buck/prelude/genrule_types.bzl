# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# A provider that's used as a marker for `genrule()`, allows dependents
# to distinguish such outputs
GenruleMarkerInfo = provider(fields = {})

GENRULE_MARKER_SUBTARGET_NAME = "genrule_marker"

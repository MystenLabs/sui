# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# NOTE: Currently, constraints can't be propagated via rule attrs and so need
# to be hard-coded here.
# We use a `read_config` to avoid hard-coding these repo-specific constraints
# into the buck2 prelude.
_CONSTRAINTS = [
    constraint.strip()
    for constraint in read_root_config("buck2", "constraint_overrides", "").split(",")
    if constraint.strip()
]

# Apparently, `==` doesn't do value comparison for `ConstraintValueInfo`, so
# impl a hacky eq impl to workaround.
def _constr_eq(a, b):
    return a.label == b.label

def _constraint_overrides_transition_impl(
        platform: PlatformInfo,
        refs: struct,
        attrs: struct) -> PlatformInfo:
    # Extract actual constraint value objects.
    new_constraints = [
        getattr(refs, constraint)[ConstraintValueInfo]
        for constraint in attrs.constraint_overrides
    ]

    # Filter out new constraints which are already a part of the platform.
    new_constraints = [
        constraint
        for constraint in new_constraints
        if (
            constraint.setting.label not in platform.configuration.constraints or
            not _constr_eq(constraint, platform.configuration.constraints[constraint.setting.label])
        )
    ]

    # Nothing to do.
    if not new_constraints:
        return platform

    # Generate new constraints.
    constraints = {}
    constraints.update(platform.configuration.constraints)
    for constraint in new_constraints:
        constraints[constraint.setting.label] = constraint

    return PlatformInfo(
        label = platform.label,
        configuration = ConfigurationInfo(
            constraints = constraints,
            values = platform.configuration.values,
        ),
    )

constraint_overrides_transition = transition(
    impl = _constraint_overrides_transition_impl,
    refs = {constraint: constraint for constraint in _CONSTRAINTS},
    attrs = [
        "constraint_overrides",
    ],
)

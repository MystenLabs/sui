# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

_SELECT_TYPE = type(select({"DEFAULT": []}))

def _is_select(thing):
    return type(thing) == _SELECT_TYPE

def _apply_helper(function, inner):
    if not _is_select(inner):
        return function(inner)
    return _apply(inner, function)

def _apply(obj, function):
    """
    If the object is a select, runs `select_map` with `function`.
    Otherwise, if the object is not a select, invokes `function` on `obj` directly.
    """
    if not _is_select(obj):
        return function(obj)
    return select_map(
        obj,
        partial(_apply_helper, function),
    )

selects = struct(
    apply = _apply,
)

# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

### Lookup rules used by BXL script (See fdb.bxl) driven by the labels below:
# 1. If target has no labels we assume a given target is a rule provider and it exposes relevant providers that BXL script depends on for a given language rule

# 2. If target has a label `dbg:info:exec=//another:target` then we assume that `ExecInfo` (see types.bzl) will be obtained via [RunInfo] of another target (//another:target).
# For example:
#    Running "buck run //another:target" (or via using [RunInfo]) should produce `ExecInfo` as its stdout

# 3. If target has a label `dbg:info:ref=//another:target` we assume a presense of //another:target which we can inspect for the presense of relevant providers (see fdb.bxl)

# This label indicates where to locate "[RunInfo]" which would output `ExecInfo` -compatible output
DBG_INFO_EXEC = "dbg:info:exec"

# This label indicates where to locate "rule provider" for a given target name. Rule providers contains language/framework
# specific information that help debugging tools to properly configure a debugger. (Support for any given language/rule needs to be implemented in fdb.bxl)
DBG_INFO_REF = "dbg:info:ref"

def dbg_info_exec(target_label) -> list[str]:
    return ["{}={}".format(DBG_INFO_EXEC, target_label)]

def dbg_info_ref(target_label) -> list[str]:
    return ["{}={}".format(DBG_INFO_REF, target_label)]

def get_info_ref(labels: list[str]) -> [str, None]:
    for label in labels:
        result = _get_value_by_mark(DBG_INFO_REF, label)
        if result:
            return result
    return None

def get_info_exec(labels: list[str]) -> [str, None]:
    for label in labels:
        result = _get_value_by_mark(DBG_INFO_EXEC, label)
        if result:
            return result
    return None

def get_label_or_mark(label: str) -> str:
    for mark in [DBG_INFO_EXEC, DBG_INFO_REF]:
        if label.startswith(mark):
            return mark
    return label

def _get_value_by_mark(mark: str, label: str) -> [str, None]:
    if label.startswith(mark + "="):
        return label.removeprefix(mark + "=")
    return None

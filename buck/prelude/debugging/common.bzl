# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Utility functions used by "fdb.bxl"

load("@prelude//debugging/types.bzl", "TargetInfo")

def target_name(node: bxl.ConfiguredTargetNode) -> str:
    return "{}:{}".format(str(node.label.path), node.label.name)

def rule_type(node: bxl.ConfiguredTargetNode) -> str:
    return node.rule_type

def create_target_info(target: bxl.ConfiguredTargetNode) -> TargetInfo:
    attrs = target.attrs_lazy()
    return TargetInfo(
        target = target_name(target),
        target_type = rule_type(target),
        labels = attrs.get("labels").value() if attrs.get("labels") != None else [],
    )

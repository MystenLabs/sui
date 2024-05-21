# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:cxx_library_utility.bzl", "cxx_attr_deps", "cxx_attr_exported_deps")
load(":apple_frameworks.bzl", "to_framework_name")

# Provider which helps to propagate the info if XCTest Swift support is needed up the target graph.
XCTestSwiftSupportInfo = provider(
    # @unsorted-dict-items
    fields = {
        "support_needed": provider_field(bool),
        # Value is unused, needed only to detect a provider type
    },
)

def xctest_swift_support_info(ctx: AnalysisContext, contains_swift_sources: bool, is_test_target: bool) -> XCTestSwiftSupportInfo:
    if contains_swift_sources and (is_test_target or _depends_on_xctest(ctx)):
        return XCTestSwiftSupportInfo(support_needed = True)
    else:
        return _inherited_xctest_swift_support_info(ctx)

def _inherited_xctest_swift_support_info(ctx: AnalysisContext) -> XCTestSwiftSupportInfo:
    all_deps = cxx_attr_deps(ctx) + cxx_attr_exported_deps(ctx)
    for dep in all_deps:
        if XCTestSwiftSupportInfo in dep:
            info = dep[XCTestSwiftSupportInfo]
            if info.support_needed:
                return info
    return XCTestSwiftSupportInfo(support_needed = False)

def _depends_on_xctest(ctx: AnalysisContext) -> bool:
    return "XCTest" in [to_framework_name(x) for x in ctx.attrs.frameworks]

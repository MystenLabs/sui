# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Helper rule which introduces proc macros into the dependency graph

load(":link_info.bzl", "RustProcMacroMarker", "RustProcMacroPlugin")

def _impl(ctx):
    # FIXME(JakobDegen): The rules themselves do not need any of the exec configured providers.
    # They only ever access the `RustProcMacroMarker` provider and do everything else through
    # plugins. However, we cannot return just that provider, since it results in
    # `buck2 build :proc_macro` building nothing. We would get the right behavior out of the
    # command line build invocation by taking a regular target dep on the aliased target and
    # returning those targets. However, that also does not work because it would result the target
    # configuration for a Rust library needing to be compatible with the target configuration for
    # its proc macro deps (a Linux-only proc macro should still be usable in a Rust build targeting
    # Mac).
    #
    # Instead, we take an exec dep on the aliased target and return those providers. This is a
    # compromise solution. The upside is that it avoids introducing any unnecessary compatibility
    # constraints. The downside is that it's still slightly wrong - because of the exec transition,
    # the returned artifacts won't correctly obey opt level/sanitizer/platform constraints in the
    # target configuration. However, this matches the behavior in fbsource today.
    #
    # Once config modifiers are implemented and roll out, it should not be too hard to get the fully
    # correct behavior
    providers = list(ctx.attrs.actual_exec.providers)
    providers.append(RustProcMacroMarker(label = ctx.attrs.actual_plugin))
    return providers

rust_proc_macro_alias = rule(
    impl = _impl,
    attrs = {
        "actual_exec": attrs.exec_dep(),
        "actual_plugin": attrs.plugin_dep(kind = RustProcMacroPlugin),
    },
)

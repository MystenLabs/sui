# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:cxx_context.bzl", "get_cxx_toolchain_info")
load("@prelude//cxx:cxx_library_utility.bzl", "cxx_is_gnu")
load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxToolchainInfo")

def _strip_debug_info(ctx: AnalysisContext, out: str, obj: Artifact) -> Artifact:
    """
    Strip debug information from an object.
    """
    strip = get_cxx_toolchain_info(ctx).binary_utilities_info.strip
    output = ctx.actions.declare_output("__stripped__", out)
    if cxx_is_gnu(ctx):
        cmd = cmd_args([strip, "--strip-debug", "--strip-unneeded", "-o", output.as_output(), obj])
    else:
        cmd = cmd_args([strip, "-S", "-o", output.as_output(), obj])
    ctx.actions.run(cmd, category = "strip_debug", identifier = out)
    return output

_InterfaceInfo = provider(fields = {
    "artifact": provider_field(typing.Any, default = None),  # "artifact"
})

def _anon_strip_debug_info_impl(ctx):
    output = _strip_debug_info(
        ctx = ctx,
        out = ctx.attrs.out,
        obj = ctx.attrs.obj,
    )
    return [DefaultInfo(), _InterfaceInfo(artifact = output)]

# Anonymous wrapper for `extract_symbol_names`.
_anon_strip_debug_info = anon_rule(
    impl = _anon_strip_debug_info_impl,
    attrs = {
        "obj": attrs.source(),
        "out": attrs.string(),
        "_cxx_toolchain": attrs.dep(providers = [CxxToolchainInfo]),
    },
    artifact_promise_mappings = {
        "strip_debug_info": lambda p: p[_InterfaceInfo].artifact,
    },
)

def strip_debug_info(
        ctx: AnalysisContext,
        out: str,
        obj: Artifact,
        anonymous: bool = False) -> Artifact:
    if anonymous:
        strip_debug_info = ctx.actions.anon_target(
            _anon_strip_debug_info,
            dict(
                _cxx_toolchain = ctx.attrs._cxx_toolchain,
                out = out,
                obj = obj,
            ),
        ).artifact("strip_debug_info")

        return ctx.actions.assert_short_path(strip_debug_info, short_path = out)
    else:
        return _strip_debug_info(
            ctx = ctx,
            out = out,
            obj = obj,
        )

def strip_object(ctx: AnalysisContext, cxx_toolchain: CxxToolchainInfo, unstripped: Artifact, strip_flags: cmd_args, category_suffix: [str, None] = None, output_path: [str, None] = None) -> Artifact:
    """
    Strip unneeded information from binaries / shared libs.
    """
    strip = cxx_toolchain.binary_utilities_info.strip

    output_path = output_path or unstripped.short_path
    stripped_lib = ctx.actions.declare_output("stripped/{}".format(output_path))

    # TODO(T109996375) support configuring the flags used for stripping
    cmd = cmd_args()
    cmd.add(strip)
    cmd.add(strip_flags)
    cmd.add([unstripped, "-o", stripped_lib.as_output()])

    effective_category_suffix = category_suffix if category_suffix else "shared_lib"
    category = "strip_{}".format(effective_category_suffix)

    ctx.actions.run(cmd, category = category, identifier = output_path)

    return stripped_lib

def strip_debug_with_gnu_debuglink(ctx: AnalysisContext, name: str, obj: Artifact) -> tuple:
    """
    Split a binary into a separate debuginfo binary and a stripped binary with a .gnu_debuglink reference
    """
    objcopy = get_cxx_toolchain_info(ctx).binary_utilities_info.objcopy

    # We flatten the directory structure because .gnu_debuglink doesn't understand directories and we
    # need to avoid name conflicts between different inputs
    debuginfo_name = name.replace("/", ".")
    debuginfo_output = ctx.actions.declare_output("__debuginfo__", debuginfo_name + ".debuginfo")
    cmd = cmd_args([objcopy, "--only-keep-debug", obj, debuginfo_output.as_output()])
    ctx.actions.run(cmd, category = "extract_debuginfo", identifier = name)

    binary_output = ctx.actions.declare_output("__stripped_objects__", name)
    cmd = cmd_args([objcopy, "--strip-debug", "--add-gnu-debuglink", debuginfo_output, obj, binary_output.as_output()])
    ctx.actions.run(cmd, category = "strip_debug", identifier = name)

    return binary_output, debuginfo_output

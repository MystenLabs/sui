# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:paths.bzl", "paths")
load(":cxx_context.bzl", "get_cxx_toolchain_info")
load(":cxx_toolchain_types.bzl", "CxxToolchainInfo")

def _shared_library_interface(
        ctx: AnalysisContext,
        output: str,
        identifier: str,
        shared_lib: [Artifact, Promise]) -> Artifact:
    """
    Convert the given shared library into an interface used for linking.
    """
    linker_info = get_cxx_toolchain_info(ctx).linker_info
    args = cmd_args(linker_info.mk_shlib_intf[RunInfo])
    args.add(shared_lib)
    output = ctx.actions.declare_output(output)
    args.add(output.as_output())
    ctx.actions.run(
        args,
        category = "generate_shared_library_interface",
        identifier = identifier,
    )
    return output

_InterfaceInfo = provider(fields = {
    "artifact": provider_field(typing.Any, default = None),  # "artifact"
})

def _anon_shared_library_interface_impl(ctx):
    output = _shared_library_interface(
        ctx = ctx,
        output = ctx.attrs.output,
        shared_lib = ctx.attrs.shared_lib,
        identifier = ctx.attrs.identifier,
    )
    return [DefaultInfo(), _InterfaceInfo(artifact = output)]

# Anonymous wrapper for `extract_symbol_names`.
_anon_shared_library_interface = anon_rule(
    impl = _anon_shared_library_interface_impl,
    attrs = {
        "identifier": attrs.option(attrs.string(), default = None),
        "output": attrs.string(),
        "shared_lib": attrs.source(),
        "_cxx_toolchain": attrs.dep(providers = [CxxToolchainInfo]),
    },
    artifact_promise_mappings = {
        "shared_library_interface": lambda p: p[_InterfaceInfo].artifact,
    },
)

def shared_library_interface(
        ctx: AnalysisContext,
        shared_lib: Artifact,
        anonymous: bool = False) -> Artifact:
    output = paths.join("__shlib_intfs__", shared_lib.short_path)

    if anonymous:
        shared_lib_interface_artifact = ctx.actions.anon_target(
            _anon_shared_library_interface,
            dict(
                _cxx_toolchain = ctx.attrs._cxx_toolchain,
                output = output,
                shared_lib = shared_lib,
                identifier = shared_lib.short_path,
            ),
        ).artifact("shared_library_interface")
        return ctx.actions.assert_short_path(shared_lib_interface_artifact, short_path = output)
    else:
        return _shared_library_interface(
            ctx = ctx,
            output = output,
            shared_lib = shared_lib,
            identifier = shared_lib.short_path,
        )

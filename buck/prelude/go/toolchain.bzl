# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

GoToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "assembler": provider_field(typing.Any, default = None),
        "cgo": provider_field(typing.Any, default = None),
        "cgo_wrapper": provider_field(typing.Any, default = None),
        "compile_wrapper": provider_field(typing.Any, default = None),
        "compiler": provider_field(typing.Any, default = None),
        "compiler_flags_shared": provider_field(typing.Any, default = None),
        "compiler_flags_static": provider_field(typing.Any, default = None),
        "cover": provider_field(typing.Any, default = None),
        "cover_srcs": provider_field(typing.Any, default = None),
        "cxx_toolchain_for_linking": provider_field(typing.Any, default = None),
        "env_go_arch": provider_field(typing.Any, default = None),
        "env_go_os": provider_field(typing.Any, default = None),
        "env_go_arm": provider_field(typing.Any, default = None),
        "env_go_root": provider_field(typing.Any, default = None),
        "external_linker_flags": provider_field(typing.Any, default = None),
        "filter_srcs": provider_field(typing.Any, default = None),
        "go": provider_field(typing.Any, default = None),
        "linker": provider_field(typing.Any, default = None),
        "linker_flags_shared": provider_field(typing.Any, default = None),
        "linker_flags_static": provider_field(typing.Any, default = None),
        "packer": provider_field(typing.Any, default = None),
        "prebuilt_stdlib": provider_field(typing.Any, default = None),
        "prebuilt_stdlib_shared": provider_field(typing.Any, default = None),
        "tags": provider_field(typing.Any, default = None),
    },
)

def get_toolchain_cmd_args(toolchain: GoToolchainInfo, go_root = True, force_disable_cgo = False) -> cmd_args:
    cmd = cmd_args("env")
    if toolchain.env_go_arch != None:
        cmd.add("GOARCH={}".format(toolchain.env_go_arch))
    if toolchain.env_go_os != None:
        cmd.add("GOOS={}".format(toolchain.env_go_os))
    if toolchain.env_go_arm != None:
        cmd.add("GOARM={}".format(toolchain.env_go_arm))
    if go_root and toolchain.env_go_root != None:
        cmd.add(cmd_args(toolchain.env_go_root, format = "GOROOT={}"))

    if force_disable_cgo:
        cmd.add("CGO_ENABLED=0")
    else:
        # CGO is enabled by default for native compilation, but we need to set it
        # explicitly for cross-builds:
        # https://go-review.googlesource.com/c/go/+/12603/2/src/cmd/cgo/doc.go
        if toolchain.cgo != None:
            cmd.add("CGO_ENABLED=1")

    return cmd

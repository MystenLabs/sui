# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# This file contains the specification for all the providers the Erlang
# integration uses.

# Information about an Erlang application and its dependencies.

ErlangAppCommonFields = [
    # application name
    "name",
    # mapping from ("application", "basename") -> to header artifact
    "includes",
    # references to ankers for the include directory
    "include_dir",
    # deps files short_path -> artifact
    "deps_files",
    # input mapping
    "input_mapping",
]

# target type to break circular dependencies
ErlangAppIncludeInfo = provider(
    fields = ErlangAppCommonFields,
)

ErlangAppInfo = provider(
    fields =
        ErlangAppCommonFields + [
            # version
            "version",

            # mapping from module name to beam artifact
            "beams",

            # for tests we need to preserve the private includes
            "private_includes",
            # mapping from name to dependency for all Erlang dependencies
            "dependencies",
            # Transitive Set for calculating the start order
            "start_dependencies",
            # reference to the .app file
            "app_file",
            # additional targets that the application depends on, the
            # default output will end up in priv/
            "resources",
            # references to ankers for the relevant directories for the application
            "priv_dir",
            "private_include_dir",
            "ebin_dir",
            # applications that are in path but not build by buck2 are virtual
            # the use-case for virtual apps are OTP applications that are shipeped
            # with the Erlang distribution
            "virtual",
            # app folders for all toolchain
            "app_folders",
            # app_folder for primary toolchain
            "app_folder",
        ],
)

ErlangReleaseInfo = provider(
    fields = {
        "name": provider_field(typing.Any, default = None),
    },
)

# toolchain provider
ErlangToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "name": provider_field(typing.Any, default = None),
        # command line erlc options used when compiling
        "erl_opts": provider_field(typing.Any, default = None),
        # emulator flags used when calling erl
        "emu_flags": provider_field(typing.Any, default = None),
        # struct containing the binaries erlc, escript, and erl
        # this is further split into local and RE
        "otp_binaries": provider_field(typing.Any, default = None),
        # utility scripts
        # building .app file
        "app_file_script": provider_field(typing.Any, default = None),
        # building escripts
        "escript_builder": provider_field(typing.Any, default = None),
        # analyzing .(h|e)rl dependencies
        "dependency_analyzer": provider_field(typing.Any, default = None),
        # trampoline rerouting stdout to stderr
        "erlc_trampoline": provider_field(typing.Any, default = None),
        # name to parse_transform artifacts mapping for core parse_transforms (that are always used) and
        # user defines ones
        "core_parse_transforms": provider_field(typing.Any, default = None),
        "parse_transforms": provider_field(typing.Any, default = None),
        # filter spec for parse transforms
        "parse_transforms_filters": provider_field(typing.Any, default = None),
        # release boot script builder
        "boot_script_builder": provider_field(typing.Any, default = None),
        # build release_variables
        "release_variables_builder": provider_field(typing.Any, default = None),
        # copying erts
        "include_erts": provider_field(typing.Any, default = None),
        # edoc-generating escript
        "edoc": provider_field(typing.Any, default = None),
        "edoc_options": provider_field(typing.Any, default = None),
        # beams we need for various reasons
        "utility_modules": provider_field(typing.Any, default = None),
        # env to be set for toolchain invocations
        "env": provider_field(typing.Any, default = None),
    },
)

# multi-version toolchain
ErlangMultiVersionToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        # toolchains
        "toolchains": provider_field(typing.Any, default = None),
        # primary toolchain
        "primary": provider_field(typing.Any, default = None),
    },
)

# OTP Binaries
ErlangOTPBinariesInfo = provider(
    fields = {
        "erl": provider_field(typing.Any, default = None),
        "erlc": provider_field(typing.Any, default = None),
        "escript": provider_field(typing.Any, default = None),
    },
)

# parse_transform
ErlangParseTransformInfo = provider(
    # @unsorted-dict-items
    fields = {
        # module implementing the parse_transform
        "source": provider_field(typing.Any, default = None),
        # potential extra files placed in a resource folder
        "extra_files": provider_field(typing.Any, default = None),
    },
)

ErlangTestInfo = provider(
    # @unsorted-dict-items
    fields =
        {
            # The name of the suite
            "name": provider_field(typing.Any, default = None),
            # mapping from name to dependency for all Erlang dependencies
            "dependencies": provider_field(typing.Any, default = None),
            # anchor to the output_dir
            "output_dir": provider_field(typing.Any, default = None),
        },
)

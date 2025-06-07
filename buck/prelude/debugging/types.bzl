# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//utils:arglike.bzl", "ArgLike")  # @unused Used as a type

# Types in this file represent a contract between a BXL script (see fdb.bxl) and internal debugging tool.
# These most likely will change as the BXL integration evolves over time to be more generic
# Right now the internal tool is making a call "buck2 bxl @mode prelude//debugging:fdb.bxl --target //some_target --arg=arg1 --arg=2"
# And fdb.bxl is expected to output artifact with serialized ExecInfo
# This information will be used to build debug configuration (primarily VSCode launch configuration but in some cases CLI invocations or Android Studio requests)

# This type represents strongly typed contract used by "fdb.bxl" script arguments.
# Even though arguments are available in "bxl_ctx", when using ctx.dynamic_output it's not possible to access arguments from there
# One way to work this around is by capturing an object in a closure and this type is used to carry the contract for this object
ScriptSettings = record(
    target = field("target_node"),
    args = field(list[ArgLike], default = []),
)

# Basic information about requested target
TargetInfo = record(
    # fully qualified target name used
    target = field(str),
    # the rule type of the target
    target_type = field(str),
    # labels on the target
    labels = field(list[str], default = []),
)

# This type mostly represent internal quirks of how python binary debugging is done.
PythonInfo = record(
    # TODO: probably unused keeping for safety
    root = field(str),
    # python script that will be used to bootstrap debugger for the "python_binary"
    entrypoint = field(str),
    # the module to load by bootstrap script
    main_module = field(str),
)

# Information required to setup debug information for CLR (Common Language Runtime) targets.
ClrInfo = record(
    entrypoint = field(str),
    args = field([list[ArgLike], None]),
    cwd = field(str),
)

# This is a generic type used to customize VSCode debugging.
# Internal VSCode debugger setup supports customizations via Jinja templates
# This type allows to extend which template is to be used and what extra parameters are needed to render it
Custom = record(
    launch_mode = field([str, None]),
    template = field(dict[str, str], default = {}),
    template_params = field(dict[str, dict[str, [str, list[str]]]]),
)

# Java DAP server requires this file in order to correctly locate classes in the source files
# The integration with a tool is available as a part of "JVM" rules. (java/kotlin_library/binary/test)
JavaInfo = record(
    classmap_file = field([Artifact, None]),
)

# Customizations that are understood by debugging tool
TargetExtraInfo = record(
    exec_info_version = field(int),
    debugger = field(str),
    # program to run under debugger
    program = field([str, None], default = None),
    # unstripped libraries path used for debugging of specific android binaries and apks
    library_search_path = field([str, None], default = None),
    # explicit args to pass to the program run under debugger
    args = field([list[ArgLike], None], default = None),
    # extra environment variables to pass to the debugger
    env = field(dict[str, str], default = {}),
    source_map = field(list[list[str]], default = []),
    python = field([PythonInfo, None], default = None),
    clr = field([ClrInfo, None], default = None),
    vscode = field([Custom, None], default = None),
    custom = field([Custom, None], default = None),
    java = field([JavaInfo, None], default = None),
)

# Full BXL response data structure understood by debugging tool
ExecInfo = record(
    target_name = field(str),
    target_info = field(TargetInfo),
    data = field([TargetExtraInfo, None], default = None),
)

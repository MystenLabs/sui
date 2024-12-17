# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Stores a reference to a Buck .NET DLL target (`Artifact`) or the name of an assembly dll (`str`)
# that can be found in the .NET framework SDK directory
DllReference = record(
    # `str` -> Path to a .NET framework DLL on the local machine.
    # `Artifact` -> Buck target dependency.
    reference = field([Artifact, str]),
)

def _args_for_dll_reference(dllref: DllReference) -> cmd_args:
    """Projects values in a `DllDepTSet` to csc.exe /reference:{dllname} arguments."""
    return cmd_args(dllref.reference, format = "/reference:{}")

# A transitive set of DLL references required to build a .NET library.
#
# The transitive set attribute `value` references the outputting assembly, and the children are a
# list of the dependencies required to build it.
DllDepTSet = transitive_set(
    args_projections = {
        # Projects "/reference:{}" arguments for `csc.exe`.
        "reference": _args_for_dll_reference,
    },
)

def generate_target_tset_children(deps: list[typing.Any], ctx: AnalysisContext) -> list[DllDepTSet]:
    """Convert a C# target's dependencies list into an array of transitive dependencies."""

    tset_children = []

    if deps:
        for dep in deps:
            if isinstance(dep, str):
                # Name of a .NET framework DLL (eg "System.Drawing.dll").
                tset_children.append(
                    ctx.actions.tset(DllDepTSet, value = DllReference(reference = dep)),
                )
            else:
                # Buck target dependency (eg "//buck/path/to:foobar").
                # Adds all of the dependencies of the Buck target dependency to the tset.
                tset_children.append(dep.get(DotNetLibraryInfo).dll_deps)

    return tset_children

DotNetLibraryInfo = provider(
    doc = "Information about a .NET library and its dependencies",
    fields = {
        # A tset of DLLs (System or Buck targets) this library depends on. The
        # `.value` is a reference to the outputting assembly artifact, and the
        # children are the dependencies required to build it.
        "dll_deps": provider_field(DllDepTSet),
        # The output file name of the library.
        "name": provider_field(str),
        # The generated .dll artifact that will need to be linked into an .exe.
        "object": provider_field(Artifact),
    },
)

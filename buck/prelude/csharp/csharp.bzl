# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(":csharp_providers.bzl", "DllDepTSet", "DllReference", "DotNetLibraryInfo", "generate_target_tset_children")
load(":toolchain.bzl", "CSharpToolchainInfo")

def csharp_library_impl(ctx: AnalysisContext) -> list[Provider]:
    toolchain = ctx.attrs._csharp_toolchain[CSharpToolchainInfo]

    # Automatically set the output dll_name to this target's name if the caller did not specify a
    # custom name.
    dll_name = "{}.dll".format(ctx.attrs.name) if not ctx.attrs.dll_name else ctx.attrs.dll_name

    # Declare that this rule will produce a dll.
    library = ctx.actions.declare_output(dll_name)

    # Create a command invoking a wrapper script that calls csc.exe to compile the .dll.
    cmd = cmd_args(toolchain.csc)

    # Add caller specified compiler flags.
    cmd.add(ctx.attrs.compiler_flags)

    # Set the output target as a .NET library.
    cmd.add("/target:library")
    cmd.add(cmd_args(
        library.as_output(),
        format = "/out:{}",
    ))

    # Don't include any default .NET framework assemblies like "mscorlib" or "System" unless
    # explicitly requested with `/reference:{}`. This flag also stops injection of other
    # default compiler flags.
    cmd.add("/noconfig")

    # Don't reference mscorlib.dll unless asked for. This is required for targets that target
    # embedded platforms such as Silverlight or WASM. (Originally for Buck1 compatibility.)
    cmd.add("/nostdlib")

    # Don't search any paths for .NET libraries unless explicitly referenced with `/lib:{}`.
    cmd.add("/nosdkpath")

    # Let csc know the directory path where it can find system assemblies. This is the path
    # that is searched by `/reference:{libname}` if `libname` is just a DLL name.
    cmd.add(cmd_args(toolchain.framework_dirs[ctx.attrs.framework_ver], format = "/lib:{}"))

    # Add a `/reference:{name}` argument for each dependency.
    # Buck target refs should be absolute paths and system assemblies just the DLL name.
    child_deps = generate_target_tset_children(ctx.attrs.deps, ctx)
    deps_tset = ctx.actions.tset(DllDepTSet, children = child_deps)

    cmd.add(deps_tset.project_as_args("reference"))

    # Specify the C# source code files that should be compiled into this target.
    # NOTE: This must happen after /out and /target!
    cmd.add(ctx.attrs.srcs)

    # Run the C# compiler to produce the output artifact.
    ctx.actions.run(cmd, category = "csharp_compile")

    return [
        DefaultInfo(default_output = library),
        DotNetLibraryInfo(
            name = ctx.attrs.dll_name,
            object = library,
            dll_deps = ctx.actions.tset(DllDepTSet, value = DllReference(reference = library), children = child_deps),
        ),
    ]

def prebuilt_dotnet_library_impl(ctx: AnalysisContext) -> list[Provider]:
    # Prebuilt libraries are just passed through since they are already built.
    return [
        DefaultInfo(default_output = ctx.attrs.assembly),
        DotNetLibraryInfo(
            name = ctx.attrs.name,
            object = ctx.attrs.assembly,
            dll_deps = ctx.actions.tset(DllDepTSet, value = DllReference(reference = ctx.attrs.assembly)),
        ),
    ]

# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxPlatformInfo")
load("@prelude//utils:platform_flavors_util.bzl", "by_platform")

# The ways that Python executables handle native linkable dependencies.
NativeLinkStrategy = enum(
    # Statically links extensions into an embedded python binary
    "native",
    # Pull transitive native deps in as fully linked standalone shared libraries.
    # This is typically the fastest build-time link strategy, as it requires no
    # top-level context and therefore can shared build artifacts with all other
    # binaries using this strategy.
    "separate",
    # Statically link all transitive native deps, which don't have an explicit
    # dep from non-C/C++ code (e.g. Python), into a monolithic shared library.
    # Native dep roots, which have an explicit dep from non-C/C++ code, remain
    # as fully linked standalone shared libraries so that, typically, application
    # code doesn't need to change to work with this strategy. This strategy
    # incurs a relatively big build-time cost, but can significantly reduce the
    # size of native code and number of shared libraries pulled into the binary.
    "merged",
)

PackageStyle = enum(
    "inplace",
    "standalone",
    "inplace_lite",
)

StripLibparStrategy = enum(
    # Strip all binaries and libraries
    "full",
    # Extract debug symbols into separate files
    "extract",
    # Leave debug symbols intact
    "none",
)

PythonToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "build_standalone_binaries_locally": provider_field(typing.Any, default = None),
        "compile": provider_field(typing.Any, default = None),
        "default_sitecustomize": provider_field(typing.Any, default = None),
        # The interpreter to use to compile bytecode.
        "host_interpreter": provider_field(typing.Any, default = None),
        "interpreter": provider_field(typing.Any, default = None),
        "version": provider_field(typing.Any, default = None),
        "native_link_strategy": provider_field(typing.Any, default = None),
        "linker_flags": provider_field(typing.Any, default = None),
        "binary_linker_flags": provider_field(typing.Any, default = None),
        "generate_static_extension_info": provider_field(typing.Any, default = None),
        "parse_imports": provider_field(typing.Any, default = None),
        "traverse_dep_manifest": provider_field(typing.Any, default = None),
        "package_style": provider_field(typing.Any, default = None),
        "strip_libpar": provider_field(typing.Any, default = None),
        "make_source_db": provider_field(typing.Any, default = None),
        "make_source_db_no_deps": provider_field(typing.Any, default = None),
        "make_py_package_inplace": provider_field(typing.Any, default = None),
        "make_py_package_standalone": provider_field(typing.Any, default = None),
        "make_py_package_manifest_module": provider_field(typing.Any, default = None),
        "make_py_package_modules": provider_field(typing.Any, default = None),
        "pex_executor": provider_field(typing.Any, default = None),
        "pex_extension": provider_field(typing.Any, default = None),
        "emit_omnibus_metadata": provider_field(typing.Any, default = None),
        "fail_with_message": provider_field(typing.Any, default = None),
        "emit_dependency_metadata": provider_field(typing.Any, default = None),
        "installer": provider_field(typing.Any, default = None),
        # A filegroup that gets added to all python executables
        "runtime_library": provider_field(Dependency | None, default = None),
        # The fully qualified name of a function that handles invoking the
        # executable's entry point
        "main_runner": provider_field(str, default = "__par__.bootstrap.run_as_main"),
    },
)

# Stores "platform"/flavor name used to resolve *platform_* arguments
PythonPlatformInfo = provider(fields = {
    "name": provider_field(typing.Any, default = None),
})

def get_package_style(ctx: AnalysisContext) -> PackageStyle:
    if ctx.attrs.package_style != None:
        return PackageStyle(ctx.attrs.package_style.lower())
    return PackageStyle(ctx.attrs._python_toolchain[PythonToolchainInfo].package_style)

def get_platform_attr(
        python_platform_info: PythonPlatformInfo,
        cxx_platform_info: CxxPlatformInfo,
        xs: list[(str, typing.Any)]) -> list[typing.Any]:
    """
    Take a platform_* value, and the non-platform version, and concat into a list
    of values based on the cxx/python platform
    """
    python_platform = python_platform_info.name
    cxx_platform = cxx_platform_info.name
    return by_platform([python_platform, cxx_platform], xs)

python = struct(
    PythonToolchainInfo = PythonToolchainInfo,
    PythonPlatformInfo = PythonPlatformInfo,
    PackageStyle = PackageStyle,
    NativeLinkStrategy = NativeLinkStrategy,
)

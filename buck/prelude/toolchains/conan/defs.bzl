# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

"""Conan C/C++ Package Manager Toolchain.

Provides a toolchain and rules to use the [Conan package manager][conan] to
manage and install third-party C/C++ dependencies.

[conan]: https://docs.conan.io/en/latest/introduction.html

## Usage

### Toolchain

First you must define a Conan toolchain, profile, and user-home in the
top-level package of the `toolchains` cell, i.e. `toolchains//:`. For example:

```
load("@prelude//toolchains/conan:defs.bzl", "conan_init", "conan_profile", "system_conan_toolchain")

system_conan_toolchain(
    name = "conan",
    conan_path = "conan",
    visibility = ["PUBLIC"],
)

conan_profile(
    name = "conan-profile",
    arch = "x86_64",
    os = "Linux" ,
    build_type = "Release",
    compiler = "gcc",
    compiler_version = "11.3",
    compiler_libcxx = "libstdc++",
)

conan_init(
    name = "conan-init",
    profile = ":conan-profile",
    visibility = ["PUBLIC"],
)
```

### Packages

Then you must define your project dependencies in a `conanfile.txt`. E.g.

```
[requires]
zlib/1.2.13
```

Then you must define targets to generate and update the Conan integration
targets. E.g.

```
load(
    "@prelude//toolchains/conan:defs.bzl",
    "conan_generate",
    "conan_lock",
    "conan_update",
    "lock_generate",
)

conan_lock(
    name = "lock",
    conanfile = "conanfile.txt",
    visibility = ["//cpp/conan/import:"],
)

lock_generate(
    name = "lock-generate",
    lockfile = ":lock",
)

conan_generate(
    name = "conan-generate",
    conanfile = "conanfile.txt",
    lockfile = ":lock",
)

conan_update(
    name = "update",
    lockfile = ":lock",
    lock_generate = ":lock-generate",
    conan_generate = ":conan-generate",
    conanfile = "conanfile.txt",
    lockfile_name = "conan.lock",
    targets_name = "conan/BUCK",
)
```

On first use, or whenever you change a Conan dependency or the toolchain
configuration you must regenerate the import targets. For example:

```
$ buck2 run //:update
```

Then you can depend on Conan provided packages defined in the generated file,
configured with the `targets_name` attribute to `conan_update`. For example:

```
cxx_binary(
    name = "main",
    srcs = ["main.cpp"],
    deps = ["//conan:zlib"],
)
```

Note, only packages that are declared as direct dependencies in the
`conanfile.txt` will have public visibility. If you wish to depend on a package
that was a transitive dependency and is currently private, then you must first
add it to the `conanfile.txt` and update the import targets.

### Example

See `examples/prelude/cpp/conan` in the Buck2 source repository for a full
working example.

## Motivation

Buck2 has the ability to build C/C++ libraries natively. However, some C/C++
projects have complex build systems and are difficult to migrate to a native
Buck2 build. Other programming languages often have established standard
package managers and such dependencies can be imported into a Buck2 project
with the help of that package manager. This module provides such an integration
for C/C++ with the help of the Conan package manager.

Conan offers a relatively large [community package set][conan-center] and is
compatible with Linux, MacOS, and Windows. It also allows for sufficient
control to support an integration into Buck2, supports toolchain configuration
and cross-compilation, and provides a Python extension API.

[conan-center]: https://conan.io/center/

## Design Goals

The Buck2 integration of Conan should fulfill the following design goals:

* The overall build should be controlled by Buck2:

    Which packages are built at which point, which compiler toolchain and
    configuration is used, where build artifacts are stored, and where
    dependencies are looked up.

    This enables the use of Buck2's own incremental build and caching
    functionality. It also enables cross-platform and cross-compilation with
    the help of Buck2's platforms and toolchains.

* Conan should provide transitive dependencies:

    The user should only have to declare the projects direct third-party C/C++
    dependencies. The transitive dependency graph, package versions, package
    downloads, and their build definitions - all these should be provided by
    Conan.

## Integration

Conan provides a number of control and integration points that are relevant to
the Buck2 integration:

* Conanfile

    A file `conanfile.txt` defines the direct dependencies of the project. This
    file is provided by the user, and used by the integration and Conan.

* Lockfile

    Conan generates a lockfile which contains the set of transitive
    dependencies, their precise versions, and their inter-dependencies. The
    integration parses this file to generate build targets for individual Conan
    packages to build in dependency order.

* Command-Line

    Conan's command-line interface can be used to request a build or fetch of
    an individual package in the context of a given lockfile. Conan will build
    only this package, provided that the package's dependencies have been built
    before and are available in Conan's cache directory. The integration uses
    this capability to build Conan packages in separate Buck2 build actions.

* Install Location

    Conan stores build artifacts and other data underneath the Conan home
    directory, which is configurable with the `CONAN_USER_HOME` environment
    variable. Package dependencies, newly built packages, and other resources
    must be available under this path. The integration configures a Conan home
    directory under Buck2's output directory and copies needed dependencies
    into place before the build and extracts relevant build results into
    dedicated output paths after the build.

* Profiles

    Conan profiles can configure the operating system and architecture to
    target or build on, the compiler and its version, and other tools and
    settings. The integration uses profiles to expose Buck2's own cxx toolchain
    and other configuration to Conan.

* Generators

    Conan is designed to integrate with other build systems, this is a
    necessity in the C/C++ ecosystem, as there is no single standard build
    system used by all projects. Conan generators can access package metadata,
    such as exposed libraries or header files, and can generate files to be
    read by another build system to import Conan built packages. Buckler is a
    Conan generator that creates Buck2 targets that import Conan built packages
    and can be depended upon by native Buck2 C/C++ targets.

"""

# TODO[AH] May prelude modules load the top-level prelude?
#   This module defines a macro that calls prebuilt_cxx_library,
#   which is provided by the prelude. Alternatively, we could change the
#   prelude to make prebuilt_cxx_library directly importable, or replace the
#   macro by a custom rule that directly constricuts the relevant providers.
load("@prelude//:prelude.bzl", "native")
load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxToolchainInfo")
load("@prelude//utils:utils.bzl", "flatten")

ConanInitInfo = provider(fields = ["profile", "user_home"])
ConanLockInfo = provider(fields = ["lockfile"])
ConanPackageInfo = provider(fields = ["reference", "package_id", "cache_out", "package_out", "cache_tset"])
ConanProfileInfo = provider(fields = ["config", "inputs"])
ConanToolchainInfo = provider(fields = ["conan"])

def _project_conan_package_dep(value: (str, Artifact)) -> cmd_args:
    """Generate dependency flags for conan_package.py"""
    return cmd_args(["--dep-reference", value[0], "--dep-cache-out", value[1]])

ConanPackageCacheTSet = transitive_set(
    args_projections = {
        "dep-flags": _project_conan_package_dep,
    },
)

def _conan_package_extract_impl(ctx: AnalysisContext) -> list[Provider]:
    conan_package_extract = ctx.attrs._conan_package_extract[RunInfo]

    cmd = cmd_args([conan_package_extract])
    sub_targets = {}

    for filename in ctx.attrs.files:
        output = ctx.actions.declare_output(filename)
        cmd.add(["--file-from", filename, "--file-to", output.as_output()])
        if filename in sub_targets:
            fail("File-name collision: " + filename)
        sub_targets[filename] = [DefaultInfo(default_outputs = [output])]

    i = 0
    for dirname in ctx.attrs.directories:
        # Some packages provide overlapping include directories, e.g.
        # `include`, and `include/jemalloc`. Such overlapping directories
        # cannot both be passed to `prebuilt_cxx_library`'s `include_dirs`.
        # This adds a counter prefix to avoid the overlap.
        prefix = str(i) + "/"
        i += 1
        output = ctx.actions.declare_output(prefix + dirname)
        cmd.add(["--directory-from", dirname, "--directory-to", output.as_output()])
        if dirname in sub_targets:
            fail("Directory-name collision: " + dirname)
        sub_targets[dirname] = [DefaultInfo(default_outputs = [output])]

    cmd.add(["--package", ctx.attrs.package[ConanPackageInfo].package_out])
    ctx.actions.run(cmd, category = "conan_extract")

    return [DefaultInfo(default_outputs = [], sub_targets = sub_targets)]

_conan_package_extract = rule(
    impl = _conan_package_extract_impl,
    attrs = {
        "directories": attrs.list(attrs.string(), doc = "Directories to extract from the package."),
        "files": attrs.list(attrs.string(), doc = "Files to extract from the package."),
        "package": attrs.dep(providers = [ConanPackageInfo], doc = "The Conan package directory to extract files from."),
        "_conan_package_extract": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:conan_package_extract"),
    },
    doc = "Extract files and directories from Conan package directory.",
)

def conan_component(
        name: str,
        defines: list[str],
        cflags: list[str],
        cppflags: list[str],
        include_paths: list[str],
        libs: list[str],
        static_libs: dict[str, list[str]],
        shared_libs: dict[str, list[str]],
        system_libs: list[str],
        deps: list[str],
        package: str):
    """Import a Conan package component.

    Extracts the relevant files from the Conan package directory and exposes
    them as a target that can be depended on by native Buck2 C/C++ targets such
    as `cxx_library`.
    """

    extract_name = name + "_extract"
    extract_tpl = ":" + extract_name + "[{}]"
    extract_include_paths = [extract_tpl.format(p) for p in include_paths]
    extract_shared_libs = {name: [extract_tpl.format(lib) for lib in libs] for name, libs in shared_libs.items()}
    extract_static_libs = {name: [extract_tpl.format(lib) for lib in libs] for name, libs in static_libs.items()}

    _conan_package_extract(
        name = extract_name,
        package = package,
        files = flatten(static_libs.values() + shared_libs.values()),
        directories = include_paths,
    )

    # [Note: Conan exported_deps] We cannot distinguish private and public
    # dependencies based on the information exposed by Conan. We default to
    # public dependencies, to avoid having to manually specify public
    # dependencies when headers need to be reexported.

    if len(libs) == 0:
        native.prebuilt_cxx_library(
            name = name,
            exported_deps = deps,  # See [Note: Conan exported_deps]
            header_dirs = extract_include_paths,
            exported_preprocessor_flags = ["-D" + d for d in defines],
            exported_lang_preprocessor_flags = {
                "c": cflags,
                "cxx": cppflags,
            },
            exported_post_linker_flags = ["-l" + lib for lib in system_libs],
        )
    elif len(libs) == 1:
        lib = libs[0]
        if lib in shared_libs:
            shared_lib = extract_shared_libs[lib][0]
        else:
            shared_lib = None
        if lib in static_libs:
            static_lib = extract_static_libs[lib][0]
        else:
            static_lib = None
        native.prebuilt_cxx_library(
            name = name,
            exported_deps = deps,  # See [Note: Conan exported_deps]
            header_dirs = extract_include_paths,
            exported_preprocessor_flags = ["-D" + d for d in defines],
            exported_lang_preprocessor_flags = {
                "c": cflags,
                "cxx": cppflags,
            },
            exported_post_linker_flags = ["-l" + lib for lib in system_libs],
            shared_lib = shared_lib,
            static_lib = static_lib,
            # TODO[AH] Can we set static_pic_lib, some libs seem to end on _pic?
            # TODO[AH] Do we need supports_merged_linking?
            # TODO[AH] Do we need supports_shared_library_interface?
        )
    else:
        # TODO[AH] Implement prebuilt_cxx_library_group.
        fail("Support for package components with multiple libraries is not yet implemented.")
        #"contacts": attrs.list(attrs.string(), default = []),
        #"default_host_platform": attrs.option(attrs.configuration_label(), default = None),
        #"deps": attrs.list(attrs.dep(), default = []),
        #"exported_deps": attrs.list(attrs.dep(), default = []),
        #"exported_platform_deps": attrs.list(attrs.tuple(attrs.regex(), attrs.set(attrs.dep(), sorted = True)), default = []),
        #"exported_preprocessor_flags": attrs.list(attrs.string(), default = []),
        #"import_libs": attrs.dict(key = attrs.string(), value = attrs.source(), sorted = False, default = {}),
        #"include_dirs": attrs.list(attrs.source(), default = []),
        #"include_in_android_merge_map_output": attrs.bool(),
        #"labels": attrs.list(attrs.string(), default = []),
        #"licenses": attrs.list(attrs.source(), default = []),
        #"provided_shared_libs": attrs.dict(key = attrs.string(), value = attrs.source(), sorted = False, default = {}),
        #"shared_libs": attrs.dict(key = attrs.string(), value = attrs.source(), sorted = False, default = {}),
        #"shared_link": attrs.list(attrs.string(), default = []),
        #"static_libs": attrs.list(attrs.source(), default = []),
        #"static_link": attrs.list(attrs.string(), default = []),
        #"static_pic_libs": attrs.list(attrs.source(), default = []),
        #"static_pic_link": attrs.list(attrs.string(), default = []),
        #"supported_platforms_regex": attrs.option(attrs.regex(), default = None),
        #"within_view": attrs.option(attrs.list(attrs.string())),

def _conan_cxx_libraries_impl(ctx: AnalysisContext) -> list[Provider]:
    default_info = DefaultInfo(
        default_outputs = ctx.attrs.main[DefaultInfo].default_outputs + flatten([c[DefaultInfo].default_outputs for c in ctx.attrs.components.values()]),
        sub_targets = {n: c.providers for n, c in ctx.attrs.components.items()},
    )
    providers = [p for p in ctx.attrs.main.providers if type(p) != "DefaultInfo"]
    providers.append(default_info)
    return providers

_conan_cxx_libraries = rule(
    impl = _conan_cxx_libraries_impl,
    attrs = {
        "components": attrs.dict(key = attrs.string(), value = attrs.dep(), doc = "The package's components."),
        "main": attrs.dep(doc = "The main package target, depends on all components."),
    },
    doc = "Helper rule to bundle Conan package components into a single target.",
)

def conan_dep(name: str, components: dict[str, str], **kwargs):
    """Bundle Conan package components into a single target.

    The target itself represents the entire Conan package, including its
    sub-components, if any. The individual components are exposed as sub-targets,
    e.g. `:openssl` represents the entire openssl package, while `:openssl[crypto]`
    represents only the `crypto` component.
    """
    native.cxx_library(
        name = "_bundle_" + name,
        exported_deps = components.values(),
    )
    _conan_cxx_libraries(
        name = name,
        main = ":_bundle_" + name,
        components = components,
        **kwargs
    )

def _conan_generate_impl(ctx: AnalysisContext) -> list[Provider]:
    conan_toolchain = ctx.attrs._conan_toolchain[ConanToolchainInfo]
    conan_init = ctx.attrs._conan_init[ConanInitInfo]
    conan_generate = ctx.attrs._conan_generate[RunInfo]

    install_folder = ctx.actions.declare_output("install-folder")
    output_folder = ctx.actions.declare_output("output-folder")
    user_home = ctx.actions.declare_output("user-home")
    manifests = ctx.actions.declare_output("manifests")
    install_info = ctx.actions.declare_output("install-info.json")
    trace_log = ctx.actions.declare_output("trace.log")
    targets_out = ctx.actions.declare_output(ctx.label.name + ".bzl")

    cmd = cmd_args([conan_generate])
    cmd.add(["--conan", conan_toolchain.conan])
    cmd.add(["--conan-init", conan_init.user_home])
    cmd.hidden(conan_init.profile.config)  # The profile is inlined in the lockfile.
    cmd.hidden(conan_init.profile.inputs)
    cmd.add(["--buckler", ctx.attrs._buckler])
    cmd.add(["--install-folder", install_folder.as_output()])
    cmd.add(["--output-folder", output_folder.as_output()])
    cmd.add(["--user-home", user_home.as_output()])
    cmd.add(["--manifests", manifests.as_output()])
    cmd.add(["--install-info", install_info.as_output()])
    cmd.add(["--trace-file", trace_log.as_output()])
    cmd.add(["--conanfile", ctx.attrs.conanfile])
    cmd.add(["--lockfile", ctx.attrs.lockfile])
    cmd.add(["--targets-out", targets_out.as_output()])
    ctx.actions.run(cmd, category = "conan_build")

    return [
        DefaultInfo(
            default_outputs = [targets_out],
            other_outputs = [
                install_folder,
                output_folder,
                user_home,
                manifests,
                install_info,
                trace_log,
            ],
        ),
    ]

conan_generate = rule(
    impl = _conan_generate_impl,
    attrs = {
        "conanfile": attrs.source(doc = "The conanfile defining the project dependencies."),
        "lockfile": attrs.source(doc = "The Conan lockfile pinning the package versions."),
        "_buckler": attrs.source(default = "prelude//toolchains/conan:buckler"),
        "_conan_generate": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:conan_generate"),
        "_conan_init": attrs.dep(providers = [ConanInitInfo], default = "toolchains//:conan-init"),
        "_conan_toolchain": attrs.default_only(attrs.toolchain_dep(default = "toolchains//:conan", providers = [ConanToolchainInfo])),
    },
    doc = "Generate Buck2 import targets for Conan packages using the Buckler generator.",
)

def _conan_init_impl(ctx: AnalysisContext) -> list[Provider]:
    conan_toolchain = ctx.attrs._conan_toolchain[ConanToolchainInfo]
    conan_init = ctx.attrs._conan_init[RunInfo]

    user_home = ctx.actions.declare_output("user-home")
    trace_log = ctx.actions.declare_output("trace.log")

    cmd = cmd_args([conan_init])
    cmd.add(["--conan", conan_toolchain.conan])
    cmd.add(["--user-home", user_home.as_output()])
    cmd.add(["--trace-file", trace_log.as_output()])
    ctx.actions.run(cmd, category = "conan_init")

    return [
        ConanInitInfo(
            user_home = user_home,
            profile = ctx.attrs.profile[ConanProfileInfo],
        ),
        DefaultInfo(default_outputs = [
            user_home,
            trace_log,
        ]),
    ]

conan_init = rule(
    impl = _conan_init_impl,
    attrs = {
        # TODO[AH] Define separate profiles for
        #   the target platform (`--profile:build`) and
        #   exec platform (`--profile:host`).
        #   This will be needed for cross-compilation.
        "profile": attrs.dep(providers = [ConanProfileInfo], doc = "The Conan profile to use."),
        "_conan_init": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:conan_init"),
        "_conan_toolchain": attrs.default_only(attrs.toolchain_dep(default = "toolchains//:conan", providers = [ConanToolchainInfo])),
    },
    doc = "Generate a Conan user-home directory.",
)

def _conan_lock_impl(ctx: AnalysisContext) -> list[Provider]:
    conan_toolchain = ctx.attrs._conan_toolchain[ConanToolchainInfo]
    conan_init = ctx.attrs._conan_init[ConanInitInfo]
    conan_lock = ctx.attrs._conan_lock[RunInfo]

    lockfile_out = ctx.actions.declare_output("conan.lock")
    user_home = ctx.actions.declare_output("user-home")
    trace_log = ctx.actions.declare_output("trace.log")

    cmd = cmd_args([conan_lock])
    cmd.add(["--conan", conan_toolchain.conan])
    cmd.add(["--conan-init", conan_init.user_home])
    cmd.add(["--profile", conan_init.profile.config])
    cmd.hidden(conan_init.profile.inputs)
    cmd.add(["--user-home", user_home.as_output()])
    cmd.add(["--trace-file", trace_log.as_output()])
    cmd.add(["--conanfile", ctx.attrs.conanfile])
    cmd.add(["--lockfile-out", lockfile_out.as_output()])
    if ctx.attrs.lockfile:
        cmd.add(["--lockfile", ctx.attrs.lockfile])
    ctx.actions.run(cmd, category = "conan_lock")

    return [
        ConanLockInfo(
            lockfile = lockfile_out,
        ),
        DefaultInfo(
            default_outputs = [lockfile_out],
            other_outputs = [user_home, trace_log],
        ),
    ]

conan_lock = rule(
    impl = _conan_lock_impl,
    attrs = {
        "conanfile": attrs.source(doc = "The conanfile defining the project dependencies."),
        "lockfile": attrs.option(attrs.source(doc = "A pre-existing lockfile to base the dependency resolution on."), default = None),
        "_conan_init": attrs.dep(providers = [ConanInitInfo], default = "toolchains//:conan-init"),
        "_conan_lock": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:conan_lock"),
        "_conan_toolchain": attrs.default_only(attrs.toolchain_dep(default = "toolchains//:conan", providers = [ConanToolchainInfo])),
    },
    doc = "Generate a Conan lock-file.",
)

def _conan_package_impl(ctx: AnalysisContext) -> list[Provider]:
    conan_toolchain = ctx.attrs._conan_toolchain[ConanToolchainInfo]
    conan_init = ctx.attrs._conan_init[ConanInitInfo]
    conan_package = ctx.attrs._conan_package[RunInfo]

    install_folder = ctx.actions.declare_output("install-folder")
    output_folder = ctx.actions.declare_output("output-folder")
    user_home = ctx.actions.declare_output("user-home")
    manifests = ctx.actions.declare_output("manifests")
    install_info = ctx.actions.declare_output("install-info.json")
    trace_log = ctx.actions.declare_output("trace.log")
    cache_out = ctx.actions.declare_output("cache-out")
    package_out = ctx.actions.declare_output("package")

    cmd = cmd_args([conan_package])
    cmd.add(["--conan", conan_toolchain.conan])
    cmd.add(["--conan-init", conan_init.user_home])
    cmd.hidden(conan_init.profile.config)  # The profile is inlined in the lockfile.
    cmd.hidden(conan_init.profile.inputs)
    cmd.add(["--lockfile", ctx.attrs.lockfile])
    cmd.add(["--reference", ctx.attrs.reference])
    cmd.add(["--package-id", ctx.attrs.package_id])
    cmd.add(["--install-folder", install_folder.as_output()])
    cmd.add(["--output-folder", output_folder.as_output()])
    cmd.add(["--user-home", user_home.as_output()])
    cmd.add(["--manifests", manifests.as_output()])
    cmd.add(["--install-info", install_info.as_output()])
    cmd.add(["--trace-file", trace_log.as_output()])
    cmd.add(["--cache-out", cache_out.as_output()])
    cmd.add(["--package-out", package_out.as_output()])

    # TODO[AH] Do we need to separate deps and build_deps?
    #   This may become necessary for cross-compilation support.
    deps = ctx.actions.tset(
        ConanPackageCacheTSet,
        children = [
            dep[ConanPackageInfo].cache_tset
            for dep in ctx.attrs.deps + ctx.attrs.build_deps
        ],
    )
    cmd.add(deps.project_as_args("dep-flags"))

    ctx.actions.run(cmd, category = "conan_build")

    return [
        ConanPackageInfo(
            reference = ctx.attrs.reference,
            package_id = ctx.attrs.package_id,
            cache_out = cache_out,
            package_out = package_out,
            cache_tset = ctx.actions.tset(ConanPackageCacheTSet, value = (ctx.attrs.reference, cache_out), children = [deps]),
        ),
        DefaultInfo(
            default_outputs = [package_out],
            other_outputs = [
                install_folder,
                output_folder,
                user_home,
                manifests,
                install_info,
                trace_log,
                cache_out,
            ],
        ),
    ]

conan_package = rule(
    impl = _conan_package_impl,
    attrs = {
        "build_deps": attrs.list(attrs.dep(providers = [ConanPackageInfo], doc = "Conan build dependencies.")),
        "deps": attrs.list(attrs.dep(providers = [ConanPackageInfo], doc = "Conan package dependencies.")),
        "lockfile": attrs.source(doc = "The Conan lockfile defining the package and its dependencies."),
        "package_id": attrs.string(doc = "The Conan package-id."),
        "reference": attrs.string(doc = "The Conan package reference <name>/<version>#<revision>."),
        "_conan_init": attrs.dep(providers = [ConanInitInfo], default = "toolchains//:conan-init"),
        "_conan_package": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:conan_package"),
        "_conan_toolchain": attrs.default_only(attrs.toolchain_dep(default = "toolchains//:conan", providers = [ConanToolchainInfo])),
    },
    doc = "Build a single Conan package.",
)

def _profile_env_var(name, value):
    # TODO[AH] Do we need `quote = "shell"` here?
    #   Setting it causes Buck2 to escape the `$PROFILE_DIR` prefix set in the
    #   very end which causes failures in Conan package builds.
    return cmd_args([name, cmd_args(value, delimiter = " ")], delimiter = "=")

def _make_wrapper_script(ctx, name, tool):
    wrapper = ctx.actions.declare_output(name)
    return ctx.actions.write(
        wrapper,
        cmd_args([
            "#!/bin/sh",
            '_SCRIPTDIR=`dirname "$0"`',
            cmd_args("exec", tool, '"$@"', delimiter = " ")
                .relative_to(wrapper, parent = 1)
                .absolute_prefix('"$_SCRIPTDIR"/'),
        ]),
        allow_args = True,
        is_executable = True,
    )

def _profile_env_tool(ctx, name, tool):
    """Create a wrapper script and assign it to the profile variable.

    Conan configures the build tools it invokes through environment variables.
    Some build tools don't accept full command-lines in the environment
    variables configuring the compiler. E.g. CMake expects `CC` to contain the
    compiler alone, not a command-line such as `zig cc`. This first creates a
    wrapper script around the provided tool to avoid build failures with tools
    that configured as full command lines.
    """
    wrapper, inputs = _make_wrapper_script(ctx, name, tool)
    return _profile_env_var(name, wrapper).hidden(tool).hidden(inputs)

def _conan_profile_impl(ctx: AnalysisContext) -> list[Provider]:
    cxx = ctx.attrs._cxx_toolchain[CxxToolchainInfo]

    content = cmd_args()

    content.add("[settings]")
    content.add(cmd_args(ctx.attrs.arch, format = "arch={}"))
    content.add(cmd_args(ctx.attrs.os, format = "os={}"))
    content.add(cmd_args(ctx.attrs.build_type, format = "build_type={}"))

    # TODO[AH] Auto-generate the compiler setting based on the toolchain.
    #   Needs a translation of CxxToolProviderType to compiler setting.
    content.add(cmd_args(ctx.attrs.compiler, format = "compiler={}"))
    content.add(cmd_args(ctx.attrs.compiler_version, format = "compiler.version={}"))
    content.add(cmd_args(ctx.attrs.compiler_libcxx, format = "compiler.libcxx={}"))

    content.add("")
    content.add("[env]")
    content.add(_profile_env_var("CMAKE_FIND_ROOT_PATH", ""))

    # TODO[AH] Define CMAKE_SYSROOT if needed.
    # TODO[AH] Define target CHOST for cross-compilation
    content.add(_profile_env_tool(ctx, "AR", cxx.linker_info.archiver))
    if cxx.as_compiler_info:
        content.add(_profile_env_tool(ctx, "AS", cxx.as_compiler_info.compiler))
        # TODO[AH] Use asm_compiler_info for Windows

    if cxx.binary_utilities_info:
        if cxx.binary_utilities_info.nm:
            content.add(_profile_env_tool(ctx, "NM", cxx.binary_utilities_info.nm))
        if cxx.binary_utilities_info.ranlib:
            content.add(_profile_env_tool(ctx, "RANLIB", cxx.binary_utilities_info.ranlib))
        if cxx.binary_utilities_info.strip:
            content.add(_profile_env_tool(ctx, "STRIP", cxx.binary_utilities_info.strip))
    if cxx.c_compiler_info:
        content.add(_profile_env_tool(ctx, "CC", cxx.c_compiler_info.compiler))
        content.add(_profile_env_var("CFLAGS", cxx.c_compiler_info.compiler_flags))
    if cxx.cxx_compiler_info:
        content.add(_profile_env_tool(ctx, "CXX", cxx.cxx_compiler_info.compiler))
        content.add(_profile_env_var("CXXFLAGS", cxx.cxx_compiler_info.compiler_flags))

    output = ctx.actions.declare_output(ctx.label.name)
    content.relative_to(output, parent = 1)
    content.absolute_prefix("$PROFILE_DIR/")
    _, args_inputs = ctx.actions.write(output, content, allow_args = True)

    return [
        DefaultInfo(default_outputs = [output]),
        ConanProfileInfo(config = output, inputs = content.hidden(args_inputs)),
    ]

conan_profile = rule(
    impl = _conan_profile_impl,
    attrs = {
        "arch": attrs.string(doc = "The target architecture"),
        "build_type": attrs.string(doc = "The Conan build-type, e.g. Release or Debug"),
        "compiler": attrs.string(doc = "The name of the C/C++ compiler, e.g. gcc, clang, or Visual Studio."),
        "compiler_libcxx": attrs.string(doc = "The C++ standard library, e.g. libstdc++, or libc++"),
        "compiler_version": attrs.string(doc = "The version of the C/C++ compiler, e.g. 12.2 for gcc, 15 for clang, or 17 for Visual Studio."),
        "os": attrs.string(doc = "The target operating system"),
        "_cxx_toolchain": attrs.default_only(attrs.toolchain_dep(default = "toolchains//:cxx", providers = [CxxToolchainInfo])),
    },
    doc = "Defines a Conan profile.",
)

def _conan_update_impl(ctx: AnalysisContext) -> list[Provider]:
    conan_update = ctx.attrs._conan_update[RunInfo]

    cmd = cmd_args([conan_update])
    cmd.add(["--update-label", str(ctx.label.raw_target())])
    cmd.add(["--lockfile", ctx.attrs.lockfile])
    cmd.add(["--lock-targets", ctx.attrs.lock_generate])
    cmd.add(["--conan-targets", ctx.attrs.conan_generate])
    cmd.add(["--conanfile", ctx.attrs.conanfile])
    cmd.add(["--lockfile-out", ctx.attrs.lockfile_name])
    cmd.add(["--targets-out", ctx.attrs.targets_name])

    return [
        DefaultInfo(default_outputs = []),
        RunInfo(args = [cmd]),
    ]

conan_update = rule(
    impl = _conan_update_impl,
    attrs = {
        "conan_generate": attrs.source(doc = "The targets generated by Buckler."),
        "conanfile": attrs.source(doc = "The Conanfile."),
        "lock_generate": attrs.source(doc = "The targets generated from the Conan lockfile."),
        "lockfile": attrs.source(doc = "The generated Conan lockfile."),
        "lockfile_name": attrs.string(doc = "Generate a lockfile with this name next to the Conanfile."),
        "targets_name": attrs.string(doc = "Generate a TARGETS file with this name next to the Conanfile."),
        "_conan_update": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:conan_update"),
    },
    doc = "Defines a runnable target that will update the Conan lockfile and import targets.",
)

def _lock_generate_impl(ctx: AnalysisContext) -> list[Provider]:
    lock_generate = ctx.attrs._lock_generate[RunInfo]

    targets_out = ctx.actions.declare_output(ctx.label.name + ".bzl")

    cmd = cmd_args([lock_generate])
    cmd.add(["--lockfile", ctx.attrs.lockfile])
    cmd.add(["--lockfile-label", str(ctx.attrs.lockfile.owner.raw_target())])
    cmd.add(["--targets-out", targets_out.as_output()])
    ctx.actions.run(cmd, category = "conan_generate")

    return [
        DefaultInfo(
            default_outputs = [targets_out],
        ),
    ]

lock_generate = rule(
    impl = _lock_generate_impl,
    attrs = {
        "lockfile": attrs.source(doc = "The Conan lockfile defining the package and its dependencies."),
        "_lock_generate": attrs.dep(providers = [RunInfo], default = "prelude//toolchains/conan:lock_generate"),
    },
    doc = "Generate targets to build individual Conan packages in dependency order based on a Conan lock-file.",
)

def _system_conan_toolchain_impl(ctx: AnalysisContext) -> list[Provider]:
    return [
        DefaultInfo(),
        ConanToolchainInfo(
            conan = RunInfo(args = [ctx.attrs.conan_path]),
        ),
    ]

system_conan_toolchain = rule(
    impl = _system_conan_toolchain_impl,
    attrs = {
        "conan_path": attrs.string(doc = "Path to the Conan executable."),
    },
    is_toolchain_rule = True,
    doc = "Uses a globally installed Conan executable.",
)

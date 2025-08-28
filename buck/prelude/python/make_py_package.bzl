# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

"""
Rule for the inplace pex builder, and some utility methods for generic pex builder
execution
"""

load("@prelude//:artifact_tset.bzl", "project_artifacts")
load("@prelude//:local_only.bzl", "package_python_locally")
load(
    "@prelude//cxx:cxx_library_utility.bzl",
    "cxx_is_gnu",
)
load(
    "@prelude//linking:link_info.bzl",
    "LinkedObject",  # @unused Used as a type
)
load("@prelude//os_lookup:defs.bzl", "OsLookup")
load("@prelude//utils:arglike.bzl", "ArgLike")
load(":compile.bzl", "PycInvalidationMode")
load(":interface.bzl", "EntryPoint", "EntryPointKind", "PythonLibraryManifestsInterface")
load(":manifest.bzl", "ManifestInfo")  # @unused Used as a type
load(":toolchain.bzl", "PackageStyle", "PythonToolchainInfo", "get_package_style")

# This represents the input to the creation of a Pex. Manifests provide source
# files, extensions are native extensions, and compile indicates whether we
# should also include bytecode from manifests.
PexModules = record(
    manifests = field(PythonLibraryManifestsInterface),
    extensions = field([ManifestInfo, None], None),
    extra_manifests = field([ManifestInfo, None], None),
    debuginfo_manifest = field([ManifestInfo, None], None),
    compile = field(bool, False),
)

# The output of pex creation. It's everything needed to make the DefaultInfo and RunInfo
# providers.
PexProviders = record(
    default_output = field(Artifact),
    other_outputs = list[(ArgLike, str)],
    other_outputs_prefix = [str, None],
    hidden_resources = list[ArgLike],
    sub_targets = dict[str, list[Provider]],
    run_cmd = cmd_args,
)

def make_py_package_providers(
        python_toolchain: PythonToolchainInfo,
        pex: PexProviders) -> list[Provider]:
    providers = [
        make_default_info(pex),
        make_run_info(pex),
    ]
    if python_toolchain.installer != None:
        providers.append(make_install_info(python_toolchain.installer, pex))
    return providers

def make_install_info(installer: ArgLike, pex: PexProviders) -> Provider:
    prefix = "{}/".format(pex.other_outputs_prefix) if pex.other_outputs_prefix != None else ""
    files = {
        "{}{}".format(prefix, path): artifact
        for artifact, path in pex.other_outputs
        if path != pex.other_outputs_prefix  # don't include prefix dir
        if path != ""  # HACK: skip artifacts without a path
    }
    files[pex.default_output.basename] = pex.default_output
    return InstallInfo(
        installer = installer,
        files = files,
    )

def make_default_info(pex: PexProviders) -> Provider:
    return DefaultInfo(
        default_output = pex.default_output,
        other_outputs = [a for a, _ in pex.other_outputs] + pex.hidden_resources,
        sub_targets = pex.sub_targets,
    )

def make_run_info(pex: PexProviders) -> Provider:
    return RunInfo(pex.run_cmd)

def _srcs(srcs: list[typing.Any], format = "{}") -> cmd_args:
    args = cmd_args()
    for src in srcs:
        args.add(cmd_args(src, format = format))
    return args

def _fail_at_build_time(
        ctx: AnalysisContext,
        python_toolchain: PythonToolchainInfo,
        msg: str) -> PexProviders:
    error_message = ctx.actions.write("__error_message", msg)
    dummy_output = ctx.actions.declare_output("__dummy_output")
    cmd = cmd_args([
        python_toolchain.fail_with_message,
        error_message,
        dummy_output.as_output(),
    ])
    ctx.actions.run(cmd, category = "par", identifier = "failure")
    return PexProviders(
        default_output = dummy_output,
        other_outputs = [],
        other_outputs_prefix = None,
        hidden_resources = [],
        sub_targets = {},
        run_cmd = cmd_args(),
    )

def _fail(
        ctx: AnalysisContext,
        python_toolchain: PythonToolchainInfo,
        suffix: str,
        msg: str) -> PexProviders:
    if suffix:
        return _fail_at_build_time(ctx, python_toolchain, msg)

    # suffix is empty, which means this is the default subtarget. All failures must
    # occur at analysis time
    fail(msg)

# TODO(nmj): Resources
# TODO(nmj): Figure out how to harmonize these flags w/ existing make_xar
#                 invocations. It might be perfectly reasonable to just have a wrapper
#                 script that invokes make_xar in a slightly different way.
def make_py_package(
        ctx: AnalysisContext,
        python_toolchain: PythonToolchainInfo,
        # A rule-provided tool to use to build the PEX.
        make_py_package_cmd: [RunInfo, None],
        package_style: PackageStyle,
        build_args: list[ArgLike],
        pex_modules: PexModules,
        shared_libraries: dict[str, (LinkedObject, bool)],
        main: EntryPoint,
        hidden_resources: [None, list[ArgLike]],
        allow_cache_upload: bool) -> PexProviders:
    """
    Passes a standardized set of flags to a `make_py_package` binary to create a python
    "executable".

    Arguments:
        - python_toolchain: Used to locate the PEX binaries.
        - package_style: How to package this binary. Might be controlled by the
          toolchain, but also by the rule.
        - build_args: Extra arguments to pass to the PEX binary.
        - pex_modules: Manifests for sources to package.
        - shared_libraries: Shared libraries to link in. Mapping of soname to
          artifact and whether they should be preloaded.
        - main: the name of the entry point to execute when running the
          resulting binary.
        - hidden_resources: extra resources the binary depends on.
    """
    srcs = []
    srcs.extend(pex_modules.manifests.src_manifests())

    if pex_modules.extensions:
        srcs.append(pex_modules.extensions.manifest)

    preload_libraries = _preload_libraries_args(ctx, shared_libraries)
    manifest_module = generate_manifest_module(ctx, python_toolchain, srcs)
    common_modules_args, dep_artifacts = _pex_modules_common_args(
        ctx,
        pex_modules,
        {name: lib for name, (lib, _) in shared_libraries.items()},
    )

    default = _make_py_package_impl(
        ctx,
        python_toolchain,
        make_py_package_cmd,
        package_style,
        build_args,
        shared_libraries,
        preload_libraries,
        common_modules_args,
        dep_artifacts,
        main,
        hidden_resources,
        manifest_module,
        pex_modules,
        output_suffix = "",
        allow_cache_upload = allow_cache_upload,
    )
    for style in PackageStyle.values():
        pex_providers = default if style == package_style.value else _make_py_package_impl(
            ctx,
            python_toolchain,
            make_py_package_cmd,
            PackageStyle(style),
            build_args,
            shared_libraries,
            preload_libraries,
            common_modules_args,
            dep_artifacts,
            main,
            hidden_resources,
            manifest_module,
            pex_modules,
            output_suffix = "-{}".format(style),
            allow_cache_upload = allow_cache_upload,
        )
        default.sub_targets[style] = make_py_package_providers(python_toolchain, pex_providers)
    return default

def _make_py_package_impl(
        ctx: AnalysisContext,
        python_toolchain: PythonToolchainInfo,
        make_py_package_cmd: [RunInfo, None],
        package_style: PackageStyle,
        build_args: list[ArgLike],
        shared_libraries: dict[str, (LinkedObject, bool)],
        preload_libraries: cmd_args,
        common_modules_args: cmd_args,
        dep_artifacts: list[(ArgLike, str)],
        main: EntryPoint,
        hidden_resources: [None, list[ArgLike]],
        manifest_module: [None, ArgLike],
        pex_modules: PexModules,
        output_suffix: str,
        allow_cache_upload: bool) -> PexProviders:
    name = "{}{}".format(ctx.attrs.name, output_suffix)
    standalone = package_style == PackageStyle("standalone")

    runtime_files = []
    if standalone and hidden_resources != None:
        # constructing this error message is expensive, only do it when we abort analysis
        error_msg = "standalone builds don't support hidden resources" if output_suffix else _hidden_resources_error_message(ctx.label, hidden_resources)

        return _fail(ctx, python_toolchain, output_suffix, error_msg)

    if not (standalone or
            package_style == PackageStyle("inplace") or
            package_style == PackageStyle("inplace_lite")):
        fail("unsupported package style: {}".format(package_style))

    symlink_tree_path = None
    if standalone:
        if python_toolchain.make_py_package_standalone == None:
            return _fail(
                ctx,
                python_toolchain,
                output_suffix,
                "Python toolchain does not provide make_py_package_standalone",
            )

    else:
        symlink_tree_path = ctx.actions.declare_output("{}#link-tree".format(name), dir = True)

    modules_args = _pex_modules_args(
        ctx,
        common_modules_args,
        dep_artifacts,
        symlink_tree_path,
        manifest_module,
        pex_modules,
        output_suffix,
    )

    output = ctx.actions.declare_output("{}{}".format(name, python_toolchain.pex_extension))

    bootstrap_args = _pex_bootstrap_args(
        python_toolchain,
        main,
        output,
        shared_libraries,
        preload_libraries,
        symlink_tree_path,
        package_style,
    )
    bootstrap_args.add(build_args)
    if standalone:
        bootstrap_args.add(ctx.attrs.standalone_build_args)
    else:
        bootstrap_args.add(ctx.attrs.inplace_build_args)

        # For inplace builds add local artifacts to outputs so they get properly materialized
        runtime_files.extend(dep_artifacts)
        runtime_files.append((symlink_tree_path, symlink_tree_path.short_path))

    # For standalone builds, or builds setting make_py_package we generate args for calling make_par.py
    if standalone or make_py_package_cmd != None:
        # We support building _standalone_ packages locally to e.g. support fbcode's
        # current style of build info stamping (e.g. T10696178).
        prefer_local = standalone and package_python_locally(ctx, python_toolchain)

        cmd = cmd_args(
            make_py_package_cmd if make_py_package_cmd != None else python_toolchain.make_py_package_standalone,
        )
        cmd.add(modules_args)
        cmd.add(bootstrap_args)
        if ctx.attrs.runtime_env:
            for k, v in ctx.attrs.runtime_env.items():
                cmd.add(cmd_args(["--passthrough", "--runtime_env={}={}".format(k, v)]))
        cmd.add(cmd_args("--no-sitecustomize"))
        identifier_prefix = "standalone{}" if standalone else "inplace{}"
        ctx.actions.run(
            cmd,
            prefer_local = prefer_local,
            category = "par",
            identifier = identifier_prefix.format(output_suffix),
            allow_cache_upload = allow_cache_upload,
        )

    else:
        modules = cmd_args(python_toolchain.make_py_package_modules)
        modules.add(modules_args)
        ctx.actions.run(modules, category = "par", identifier = "modules{}".format(output_suffix))

        bootstrap = cmd_args(python_toolchain.make_py_package_inplace)
        bootstrap.add(bootstrap_args)
        if ctx.attrs.runtime_env:
            for k, v in ctx.attrs.runtime_env.items():
                bootstrap.add(cmd_args(["--runtime_env", "{}={}".format(k, v)]))

        ctx.actions.run(bootstrap, category = "par", identifier = "bootstrap{}".format(output_suffix))

    run_args = []

    # Windows can't run PAR directly.
    if ctx.attrs._exec_os_type[OsLookup].platform == "windows":
        run_args.append(ctx.attrs._python_toolchain[PythonToolchainInfo].interpreter)
    run_args.append(output)

    if hidden_resources == None:
        hidden_resources = []

    return PexProviders(
        default_output = output,
        other_outputs = runtime_files,
        other_outputs_prefix = symlink_tree_path.short_path if symlink_tree_path != None else None,
        hidden_resources = hidden_resources,
        sub_targets = {},
        run_cmd = cmd_args(run_args).hidden([a for a, _ in runtime_files] + hidden_resources),
    )

def _preload_libraries_args(ctx: AnalysisContext, shared_libraries: dict[str, (LinkedObject, bool)]) -> cmd_args:
    preload_libraries_path = ctx.actions.write(
        "__preload_libraries.txt",
        cmd_args([
            "--preload={}".format(name)
            for name, (_, preload) in shared_libraries.items()
            if preload
        ]),
    )
    return cmd_args(preload_libraries_path, format = "@{}")

def _pex_bootstrap_args(
        toolchain: PythonToolchainInfo,
        main: EntryPoint,
        output: Artifact,
        shared_libraries: dict[str, (LinkedObject, bool)],
        preload_libraries: cmd_args,
        symlink_tree_path: Artifact | None,
        package_style: PackageStyle) -> cmd_args:
    cmd = cmd_args()
    cmd.add(preload_libraries)
    cmd.add([
        "--python",
        toolchain.interpreter,
        "--host-python",
        toolchain.host_interpreter,
    ])
    if main[0] == EntryPointKind("module"):
        cmd.add(["--entry-point", main[1]])
    else:
        cmd.add(["--main-function", main[1]])
    if symlink_tree_path != None:
        cmd.add(cmd_args(["--modules-dir", symlink_tree_path]).ignore_artifacts())

    if toolchain.main_runner:
        cmd.add(["--main-runner", toolchain.main_runner])

    # Package style `inplace_lite` cannot be used with shared libraries
    if package_style == PackageStyle("inplace_lite") and not shared_libraries:
        cmd.add("--use-lite")
    cmd.add(output.as_output())

    return cmd

def _pex_modules_common_args(
        ctx: AnalysisContext,
        pex_modules: PexModules,
        shared_libraries: dict[str, LinkedObject]) -> (cmd_args, list[(ArgLike, str)]):
    srcs = []
    src_artifacts = []
    deps = []

    srcs.extend(pex_modules.manifests.src_manifests())
    src_artifacts.extend(pex_modules.manifests.src_artifacts_with_paths())

    if pex_modules.extensions:
        srcs.append(pex_modules.extensions.manifest)
        src_artifacts.extend(pex_modules.extensions.artifacts)

    if pex_modules.extra_manifests:
        srcs.append(pex_modules.extra_manifests.manifest)
        src_artifacts.extend(pex_modules.extra_manifests.artifacts)

    deps.extend(src_artifacts)
    resources = pex_modules.manifests.resource_manifests()
    deps.extend(pex_modules.manifests.resource_artifacts_with_paths())

    src_manifests_path = ctx.actions.write(
        "__src_manifests.txt",
        _srcs(srcs, format = "--module-manifest={}"),
    )
    resource_manifests_path = ctx.actions.write(
        "__resource_manifests.txt",
        _srcs(resources, format = "--resource-manifest={}"),
    )

    native_libraries = [s.output for s in shared_libraries.values()]
    native_library_srcs_path = ctx.actions.write(
        "__native_libraries___srcs.txt",
        _srcs(native_libraries, format = "--native-library-src={}"),
    )
    native_library_dests_path = ctx.actions.write(
        "__native_libraries___dests.txt",
        ["--native-library-dest={}".format(lib) for lib in shared_libraries],
    )

    src_manifest_args = cmd_args(src_manifests_path).hidden(srcs)
    resource_manifest_args = cmd_args(resource_manifests_path).hidden(resources)
    native_library_srcs_args = cmd_args(native_library_srcs_path)

    cmd = cmd_args()
    cmd.add(cmd_args(src_manifest_args, format = "@{}"))
    cmd.add(cmd_args(resource_manifest_args, format = "@{}"))
    cmd.add(cmd_args(native_library_srcs_args, format = "@{}"))
    cmd.add(cmd_args(native_library_dests_path, format = "@{}"))

    if pex_modules.debuginfo_manifest:
        debuginfo_files = pex_modules.debuginfo_manifest.artifacts
        debuginfo_srcs_path = ctx.actions.write(
            "__debuginfo___srcs.txt",
            _srcs([src for src, _ in debuginfo_files], format = "--debuginfo-src={}"),
        )
        debuginfo_srcs_args = cmd_args(debuginfo_srcs_path)
        cmd.add(cmd_args(debuginfo_srcs_args, format = "@{}"))
        deps.extend(debuginfo_files)

    if ctx.attrs.package_split_dwarf_dwp:
        if ctx.attrs.strip_libpar == "extract" and get_package_style(ctx) == PackageStyle("standalone") and cxx_is_gnu(ctx):
            # rename to match extracted debuginfo package
            dwp = [(s.dwp, "{}.debuginfo.dwp".format(n)) for n, s in shared_libraries.items() if s.dwp != None]
        else:
            dwp = [(s.dwp, "{}.dwp".format(n)) for n, s in shared_libraries.items() if s.dwp != None]
        dwp_srcs_path = ctx.actions.write(
            "__dwp___srcs.txt",
            _srcs([src for src, _ in dwp], format = "--dwp-src={}"),
        )
        dwp_dests_path = ctx.actions.write(
            "__dwp___dests.txt",
            _srcs([dest for _, dest in dwp], format = "--dwp-dest={}"),
        )
        dwp_srcs_args = cmd_args(dwp_srcs_path)
        cmd.add(cmd_args(dwp_srcs_args, format = "@{}"))
        cmd.add(cmd_args(dwp_dests_path, format = "@{}"))

        deps.extend(dwp)

    deps.extend([(lib.output, name) for name, lib in shared_libraries.items()])

    external_debug_info = project_artifacts(
        ctx.actions,
        [lib.external_debug_info for lib in shared_libraries.values()],
    )

    # HACK: exclude external_debug_info from InstallInfo by providing an empty path
    deps.extend([(d, "") for d in external_debug_info])

    return (cmd, deps)

def _pex_modules_args(
        ctx: AnalysisContext,
        common_args: cmd_args,
        dep_artifacts: list[(ArgLike, str)],
        symlink_tree_path: [None, Artifact],
        manifest_module: [ArgLike, None],
        pex_modules: PexModules,
        output_suffix: str) -> cmd_args:
    """
    Produces args to deal with a PEX's modules. Returns args to pass to the
    modules builder, and artifacts the resulting modules would require at
    runtime (this might be empty for e.g. a standalone pex).
    """

    cmd = cmd_args()
    cmd.add(common_args)

    if manifest_module != None:
        cmd.add(cmd_args(manifest_module, format = "--module-manifest={}"))

    if pex_modules.compile:
        pyc_mode = PycInvalidationMode("UNCHECKED_HASH") if symlink_tree_path == None else PycInvalidationMode("CHECKED_HASH")
        bytecode_manifests = pex_modules.manifests.bytecode_manifests(pyc_mode)
        dep_artifacts.extend(pex_modules.manifests.bytecode_artifacts_with_paths(pyc_mode))

        bytecode_manifests_path = ctx.actions.write(
            "__bytecode_manifests{}.txt".format(output_suffix),
            _srcs(
                bytecode_manifests,
                format = "--module-manifest={}",
            ),
        )
        cmd.add(cmd_args(bytecode_manifests_path, format = "@{}"))
        cmd.hidden(bytecode_manifests)

    if symlink_tree_path != None:
        cmd.add(["--modules-dir", symlink_tree_path.as_output()])
    else:
        # Accumulate all the artifacts we depend on. Only add them to the command
        # if we are not going to create symlinks.
        cmd.hidden([a for a, _ in dep_artifacts])

    return cmd

def _hidden_resources_error_message(current_target: Label, hidden_resources) -> str:
    """
    Friendlier error message about putting non-python resources into standalone bins
    """
    owner_to_artifacts = {}

    for resource_set in hidden_resources:
        for resources in resource_set.traverse():
            for r in resources:
                # TODO: `r` is sometimes a tset projection, but it shouldn't be
                if getattr(r, "is_source", True):
                    # Source files; do a string repr so that we get the
                    # package path in there too
                    owner_to_artifacts.setdefault("", []).append(str(r))
                else:
                    owner_to_artifacts.setdefault(r.owner, []).append(r.short_path)

    msg = (
        "Cannot package hidden srcs/resources in a standalone python_binary. " +
        'Eliminate resources in non-Python dependencies of this python binary, use `package_style = "inplace"`, ' +
        'use `strip_mode="full"` or turn off Split DWARF `-c fbcode.split-dwarf=false` on C++ binary resources.\n'
    )

    for (rule, resources) in owner_to_artifacts.items():
        if rule != "":
            msg += "Hidden srcs/resources for {}\n".format(rule)
        else:
            msg += "Source files:\n"
            msg += "Find the reason this file was included with `buck2 cquery 'allpaths({}, owner(%s))' <file paths>`\n".format(current_target.raw_target())
        for resource in sorted(resources):
            msg += "  {}\n".format(resource)
    return msg

def generate_manifest_module(
        ctx: AnalysisContext,
        python_toolchain: PythonToolchainInfo,
        src_manifests: list[ArgLike]) -> [ArgLike, None]:
    """
    Generates a __manifest__.py module, and an extra entry to add to source manifests.

    The contents of the manifest are taken from an attribute if it exists. If the
    attribute is None, this function does nothing.
    """

    if ctx.attrs.manifest_module_entries == None:
        return None
    module = ctx.actions.declare_output("manifest/__manifest__.py")
    entries_json = ctx.actions.write_json("manifest/entries.json", ctx.attrs.manifest_module_entries)
    src_manifests_path = ctx.actions.write(
        "__module_manifests.txt",
        _srcs(src_manifests, format = "--module-manifest={}"),
    )
    cmd = cmd_args(python_toolchain.make_py_package_manifest_module)
    cmd.add(["--manifest-entries", entries_json])
    cmd.add(cmd_args(src_manifests_path, format = "@{}"))
    cmd.hidden(src_manifests)
    cmd.add(["--output", module.as_output()])
    ctx.actions.run(cmd, category = "par", identifier = "manifest-module")

    json_entries_output = ctx.actions.declare_output("manifest/__manifest__.json")
    ctx.actions.copy_file(json_entries_output.as_output(), entries_json)

    src_manifest = ctx.actions.write_json(
        "manifest/module_manifest.json",
        [
            ["__manifest__.py", module, "prelude//python:make_py_package.bzl"],
            ["__manifest__.json", json_entries_output, "prelude//python:make_py_package.bzl"],
        ],
        with_inputs = True,
    )

    return src_manifest

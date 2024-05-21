# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:artifact_tset.bzl", "project_artifacts")
load("@prelude//:local_only.bzl", "link_cxx_binary_locally")
load("@prelude//:paths.bzl", "paths")
load("@prelude//:resources.bzl", "create_resource_db", "gather_resources")
load("@prelude//cxx:cxx_context.bzl", "get_cxx_toolchain_info")
load("@prelude//cxx:cxx_library_utility.bzl", "cxx_attr_deps")
load(
    "@prelude//cxx:cxx_link_utility.bzl",
    "executable_shared_lib_arguments",
    "make_link_args",
)
load("@prelude//cxx:cxx_toolchain_types.bzl", "LinkerInfo")
load("@prelude//cxx:debug.bzl", "SplitDebugMode")
load("@prelude//cxx:dwp.bzl", "dwp", "dwp_available")
load(
    "@prelude//cxx:linker.bzl",
    "get_default_shared_library_name",
    "get_shared_library_name_linker_flags",
)
load(
    "@prelude//linking:link_info.bzl",
    "LinkArgs",
    "LinkStyle",
    "get_link_args_for_strategy",
    "to_link_strategy",
)
load(
    "@prelude//linking:shared_libraries.bzl",
    "merge_shared_libraries",
    "traverse_shared_library_info",
)
load("@prelude//os_lookup:defs.bzl", "OsLookup")
load("@prelude//utils:cmd_script.bzl", "ScriptOs", "cmd_script")
load("@prelude//utils:set.bzl", "set")
load("@prelude//utils:utils.bzl", "flatten_dict")
load(
    ":build_params.bzl",
    "BuildParams",  # @unused Used as a type
    "CrateType",
    "Emit",
    "crate_type_codegen",
    "crate_type_linked",
    "emit_needs_codegen",
    "output_filename",
)
load(":context.bzl", "CommonArgsInfo", "CompileContext")
load(":extern.bzl", "crate_map_arg", "extern_arg")
load(
    ":failure_filter.bzl",
    "RustFailureFilter",
    "failure_filter",
)
load(
    ":link_info.bzl",
    "CrateName",  #@unused Used as a type
    "RustCxxLinkGroupInfo",  #@unused Used as a type
    "RustLinkInfo",
    "RustLinkStyleInfo",
    "attr_crate",
    "attr_simple_crate_for_filenames",
    "get_available_proc_macros",
    "inherited_external_debug_info",
    "inherited_merged_link_infos",
    "inherited_shared_libs",
    "normalize_crate",
    "resolve_rust_deps",
    "style_info",
)
load(":resources.bzl", "rust_attr_resources")
load(":rust_toolchain.bzl", "RustToolchainInfo", "ctx_toolchain_info")

RustcOutput = record(
    output = field(Artifact),
    diag = field(dict[str, Artifact]),
    pdb = field([Artifact, None]),
    dwp_output = field([Artifact, None]),
    # Zero or more Split DWARF debug info files are emitted into this directory
    # with unpredictable filenames.
    dwo_output_directory = field([Artifact, None]),
)

def compile_context(ctx: AnalysisContext) -> CompileContext:
    toolchain_info = ctx_toolchain_info(ctx)
    cxx_toolchain_info = get_cxx_toolchain_info(ctx)

    # Setup source symlink tree.
    srcs = {src.short_path: src for src in ctx.attrs.srcs}
    srcs.update({k: v for v, k in ctx.attrs.mapped_srcs.items()})

    # Decide whether to use symlinked_dir or copied_dir.
    #
    # If a source is a prefix of any other source, use copied_dir. This supports
    # e.g. `srcs = [":foo.crate"]` where :foo.crate is an http_archive, together
    # with a `mapped_srcs` which overlays additional generated files into that
    # directory. Symlinked_dir would error in this situation.
    prefixes = {}
    symlinked_srcs = None
    for src in sorted(srcs.keys(), key = len, reverse = True):
        if src in prefixes:
            symlinked_srcs = ctx.actions.copied_dir("__srcs", srcs)
            break
        components = src.split("/")
        for i in range(1, len(components)):
            prefixes["/".join(components[:i])] = None
    if not symlinked_srcs:
        symlinked_srcs = ctx.actions.symlinked_dir("__srcs", srcs)

    linker = _linker_args(ctx, cxx_toolchain_info.linker_info)
    clippy_wrapper = _clippy_wrapper(ctx, toolchain_info)

    return CompileContext(
        toolchain_info = toolchain_info,
        cxx_toolchain_info = cxx_toolchain_info,
        symlinked_srcs = symlinked_srcs,
        linker_args = linker,
        clippy_wrapper = clippy_wrapper,
        common_args = {},
        flagfiles_for_extern = {},
        flagfiles_for_crate_map = {},
        transitive_dependency_dirs = {},
    )

def generate_rustdoc(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        # link style doesn't matter, but caller should pass in build params
        # with static-pic (to get best cache hits for deps)
        params: BuildParams,
        default_roots: list[str],
        document_private_items: bool) -> Artifact:
    exec_is_windows = ctx.attrs._exec_os_type[OsLookup].platform == "windows"

    toolchain_info = compile_ctx.toolchain_info

    common_args = _compute_common_args(
        ctx = ctx,
        compile_ctx = compile_ctx,
        # to make sure we get the rmeta's generated for the crate dependencies,
        # rather than full .rlibs
        emit = Emit("metadata"),
        params = params,
        dep_link_style = params.dep_link_style,
        default_roots = default_roots,
        is_rustdoc_test = False,
    )

    subdir = common_args.subdir + "-rustdoc"
    output = ctx.actions.declare_output(subdir)

    plain_env, path_env = _process_env(compile_ctx, ctx.attrs.env, exec_is_windows)

    rustdoc_cmd = cmd_args(
        [cmd_args("--env=", k, "=", v, delimiter = "") for k, v in plain_env.items()],
        [cmd_args("--path-env=", k, "=", v, delimiter = "") for k, v in path_env.items()],
        cmd_args(str(ctx.label.raw_target()), format = "--env=RUSTDOC_BUCK_TARGET={}"),
        toolchain_info.rustdoc,
        toolchain_info.rustdoc_flags,
        ctx.attrs.rustdoc_flags,
        common_args.args,
        cmd_args(output.as_output(), format = "--out-dir={}"),
    )

    if document_private_items:
        rustdoc_cmd.add("--document-private-items")

    url_prefix = toolchain_info.extern_html_root_url_prefix
    if url_prefix != None:
        # Flag --extern-html-root-url used below is only supported on nightly.
        rustdoc_cmd.add("-Zunstable-options")

        for dep in resolve_rust_deps(ctx):
            if dep.label.cell != ctx.label.cell:
                # TODO: support a different extern_html_root_url_prefix per cell
                continue

            if dep.name:
                name = normalize_crate(dep.name)
            else:
                name = dep.info.crate

            rustdoc_cmd.add(
                "--extern-html-root-url={}={}/{}:{}"
                    .format(name, url_prefix, dep.label.package, dep.label.name),
            )

    rustdoc_cmd.hidden(toolchain_info.rustdoc, compile_ctx.symlinked_srcs)

    rustdoc_cmd = _long_command(
        ctx = ctx,
        exe = toolchain_info.rustc_action,
        args = rustdoc_cmd,
        argfile_name = "{}.args".format(subdir),
    )

    ctx.actions.run(rustdoc_cmd, category = "rustdoc")

    return output

def generate_rustdoc_test(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        link_style: LinkStyle,
        library: RustLinkStyleInfo,
        params: BuildParams,
        default_roots: list[str]) -> (cmd_args, dict[str, cmd_args]):
    exec_is_windows = ctx.attrs._exec_os_type[OsLookup].platform == "windows"

    toolchain_info = compile_ctx.toolchain_info
    native_unbundle_deps = toolchain_info.native_unbundle_deps

    resources = create_resource_db(
        ctx = ctx,
        name = "doctest/resources.json",
        binary = library.rlib,
        resources = flatten_dict(gather_resources(
            label = ctx.label,
            resources = rust_attr_resources(ctx),
            deps = cxx_attr_deps(ctx),
        ).values()),
    )

    # Gather and setup symlink tree of transitive shared library deps.
    shared_libs = {}
    if link_style == LinkStyle("shared"):
        shlib_info = merge_shared_libraries(
            ctx.actions,
            deps = inherited_shared_libs(ctx, native_unbundle_deps, include_doc_deps = True),
        )
        for soname, shared_lib in traverse_shared_library_info(shlib_info).items():
            shared_libs[soname] = shared_lib.lib
    extra_link_args, runtime_files, _ = executable_shared_lib_arguments(
        ctx.actions,
        compile_ctx.cxx_toolchain_info,
        resources,
        shared_libs,
    )

    common_args = _compute_common_args(
        ctx = ctx,
        compile_ctx = compile_ctx,
        emit = Emit("link"),
        params = params,
        dep_link_style = params.dep_link_style,
        default_roots = default_roots,
        is_rustdoc_test = True,
    )

    link_args_output = make_link_args(
        ctx.actions,
        compile_ctx.cxx_toolchain_info,
        [
            LinkArgs(flags = extra_link_args),
            get_link_args_for_strategy(
                ctx,
                inherited_merged_link_infos(ctx, native_unbundle_deps, include_doc_deps = True),
                # TODO(cjhopman): It's unclear how rust is using link_style. I'm not sure if it's intended to be a LibOutputStyle or a LinkStrategy.
                to_link_strategy(link_style),
            ),
        ],
        "{}-{}".format(common_args.subdir, common_args.tempfile),
    )

    link_args_output.link_args.add(ctx.attrs.doc_linker_flags or [])

    linker_argsfile, _ = ctx.actions.write(
        "{}/__{}_linker_args.txt".format(common_args.subdir, common_args.tempfile),
        link_args_output.link_args,
        allow_args = True,
    )

    if exec_is_windows:
        runtool = ["--runtool=cmd.exe", "--runtool-arg=/V:OFF", "--runtool-arg=/C"]
    else:
        runtool = ["--runtool=/usr/bin/env"]

    rustdoc_cmd = cmd_args(
        "--test",
        "-Zunstable-options",
        cmd_args("--test-builder=", toolchain_info.compiler, delimiter = ""),
        toolchain_info.rustdoc_flags,
        ctx.attrs.rustdoc_flags,
        common_args.args,
        extern_arg(ctx, compile_ctx, [], attr_crate(ctx), library.rlib),
        "--extern=proc_macro" if ctx.attrs.proc_macro else [],
        compile_ctx.linker_args,
        cmd_args(linker_argsfile, format = "-Clink-arg=@{}"),
        runtool,
        cmd_args(toolchain_info.rustdoc_test_with_resources, format = "--runtool-arg={}"),
        cmd_args("--runtool-arg=--resources=", resources, delimiter = ""),
        "--color=always",
        "--test-args=--color=always",
    )

    rustdoc_cmd.hidden(compile_ctx.symlinked_srcs, link_args_output.hidden, runtime_files)

    rustdoc_cmd = _long_command(
        ctx = ctx,
        exe = toolchain_info.rustdoc,
        args = rustdoc_cmd,
        argfile_name = "{}.args".format(common_args.subdir),
    )

    plain_env, path_env = _process_env(compile_ctx, ctx.attrs.env, exec_is_windows)
    rustdoc_env = plain_env | path_env

    # Pass everything in env + doc_env, except ones with value None in doc_env.
    for k, v in ctx.attrs.doc_env.items():
        if v == None:
            rustdoc_env.pop(k, None)
        else:
            rustdoc_env[k] = cmd_args(v)
    rustdoc_env["RUSTC_BOOTSTRAP"] = cmd_args("1")  # for `-Zunstable-options`

    return (rustdoc_cmd, rustdoc_env)

# Generate multiple compile artifacts so that distinct sets of artifacts can be
# generated concurrently.
def rust_compile_multi(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        emits: list[Emit],
        params: BuildParams,
        dep_link_style: LinkStyle,
        default_roots: list[str],
        extra_link_args: list[typing.Any] = [],
        predeclared_outputs: dict[Emit, Artifact] = {},
        extra_flags: list[[str, ResolvedStringWithMacros]] = [],
        is_binary: bool = False,
        allow_cache_upload: bool = False,
        rust_cxx_link_group_info: [RustCxxLinkGroupInfo, None] = None) -> list[RustcOutput]:
    outputs = []

    for emit in emits:
        outs = rust_compile(
            ctx = ctx,
            compile_ctx = compile_ctx,
            emit = emit,
            params = params,
            dep_link_style = dep_link_style,
            default_roots = default_roots,
            extra_link_args = extra_link_args,
            predeclared_outputs = predeclared_outputs,
            extra_flags = extra_flags,
            is_binary = is_binary,
            allow_cache_upload = allow_cache_upload,
            rust_cxx_link_group_info = rust_cxx_link_group_info,
        )
        outputs.append(outs)

    return outputs

# Generate a compilation action. A single instance of rustc can emit
# numerous output artifacts, so return an artifact object for each of
# them.
def rust_compile(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        emit: Emit,
        params: BuildParams,
        dep_link_style: LinkStyle,
        default_roots: list[str],
        extra_link_args: list[typing.Any] = [],
        predeclared_outputs: dict[Emit, Artifact] = {},
        extra_flags: list[[str, ResolvedStringWithMacros]] = [],
        is_binary: bool = False,
        allow_cache_upload: bool = False,
        rust_cxx_link_group_info: [RustCxxLinkGroupInfo, None] = None) -> RustcOutput:
    exec_is_windows = ctx.attrs._exec_os_type[OsLookup].platform == "windows"

    toolchain_info = compile_ctx.toolchain_info
    native_unbundle_deps = toolchain_info.native_unbundle_deps

    lints, clippy_lints = _lint_flags(compile_ctx)

    common_args = _compute_common_args(
        ctx = ctx,
        compile_ctx = compile_ctx,
        emit = emit,
        params = params,
        dep_link_style = dep_link_style,
        default_roots = default_roots,
        is_rustdoc_test = False,
    )

    path_sep = "\\" if exec_is_windows else "/"
    rustc_cmd = cmd_args(
        # Lints go first to allow other args to override them.
        lints,
        # Report unused --extern crates in the notification stream.
        ["--json=unused-externs-silent", "-Wunused-crate-dependencies"] if toolchain_info.report_unused_deps else [],
        "--json=artifacts",  # only needed for pipeline but no harm in always leaving it enabled
        common_args.args,
        cmd_args("--remap-path-prefix=", compile_ctx.symlinked_srcs, path_sep, "=", ctx.label.path, path_sep, delimiter = ""),
        compile_ctx.linker_args,
        extra_flags,
    )

    # If we're using failure filtering then we need to make sure the final
    # artifact location is the predeclared one since its specific path may have
    # already been encoded into the other compile args (eg rpath). So we still
    # let rustc_emit generate its own output artifacts, and then make sure we
    # use the predeclared one as the output after the failure filter action
    # below. Otherwise we'll use the predeclared outputs directly.
    if toolchain_info.failure_filter:
        emit_output, emit_args, extra_out = _rustc_emit(
            ctx = ctx,
            compile_ctx = compile_ctx,
            emit = emit,
            predeclared_outputs = {},
            subdir = common_args.subdir,
            params = params,
        )
    else:
        emit_output, emit_args, extra_out = _rustc_emit(
            ctx = ctx,
            compile_ctx = compile_ctx,
            emit = emit,
            predeclared_outputs = predeclared_outputs,
            subdir = common_args.subdir,
            params = params,
        )

    pdb_artifact = None
    dwp_inputs = []
    if crate_type_linked(params.crate_type) and not common_args.is_check:
        subdir = common_args.subdir
        tempfile = common_args.tempfile

        # If this crate type has an associated native dep link style, include deps
        # of that style.

        if rust_cxx_link_group_info:
            inherited_link_args = LinkArgs(
                infos = rust_cxx_link_group_info.filtered_links + [rust_cxx_link_group_info.symbol_files_info],
            )

        else:
            inherited_link_args = get_link_args_for_strategy(
                ctx,
                inherited_merged_link_infos(
                    ctx,
                    native_unbundle_deps,
                    include_doc_deps = False,
                ),
                # TODO(cjhopman): It's unclear how rust is using link_style. I'm not sure if it's intended to be a LibOutputStyle or a LinkStrategy.
                to_link_strategy(dep_link_style),
            )

        link_args_output = make_link_args(
            ctx.actions,
            compile_ctx.cxx_toolchain_info,
            [
                LinkArgs(flags = extra_link_args),
                inherited_link_args,
            ],
            "{}-{}".format(subdir, tempfile),
            output_short_path = emit_output.short_path,
        )
        linker_argsfile, _ = ctx.actions.write(
            "{}/__{}_linker_args.txt".format(subdir, tempfile),
            link_args_output.link_args,
            allow_args = True,
        )

        pdb_artifact = link_args_output.pdb_artifact
        dwp_inputs = [link_args_output.link_args]
        rustc_cmd.add(cmd_args(linker_argsfile, format = "-Clink-arg=@{}"))
        rustc_cmd.hidden(link_args_output.hidden)

    (diag, build_status) = _rustc_invoke(
        ctx = ctx,
        compile_ctx = compile_ctx,
        prefix = "{}/{}".format(common_args.subdir, common_args.tempfile),
        rustc_cmd = cmd_args(toolchain_info.compiler, rustc_cmd, emit_args),
        diag = "diag",
        required_outputs = [emit_output],
        short_cmd = common_args.short_cmd,
        is_binary = is_binary,
        allow_cache_upload = allow_cache_upload,
        crate_map = common_args.crate_map,
        only_artifact = "metadata" if toolchain_info.pipelined and emit == Emit("metadata") else None,
    )

    # Add clippy diagnostic targets for check builds
    if common_args.is_check:
        # We don't really need the outputs from this build, just to keep the artifact accounting straight
        clippy_out, clippy_emit_args, _extra_out = _rustc_emit(
            ctx = ctx,
            compile_ctx = compile_ctx,
            emit = emit,
            predeclared_outputs = {},
            subdir = common_args.subdir + "-clippy",
            params = params,
        )
        clippy_env = dict()
        if toolchain_info.clippy_toml:
            # Clippy wants to be given a path to a directory containing a
            # clippy.toml (or .clippy.toml). Our buckconfig accepts an arbitrary
            # label like //path/to:my-clippy.toml which may not have the
            # filename that clippy looks for. Here we make a directory that
            # symlinks the requested configuration file under the required name.
            clippy_conf_dir = ctx.actions.symlinked_dir(
                common_args.subdir + "-clippy-configuration",
                {"clippy.toml": toolchain_info.clippy_toml},
            )
            clippy_env["CLIPPY_CONF_DIR"] = clippy_conf_dir
        (clippy_diag, _) = _rustc_invoke(
            ctx = ctx,
            compile_ctx = compile_ctx,
            prefix = "{}/{}".format(common_args.subdir, common_args.tempfile),
            # Lints go first to allow other args to override them.
            rustc_cmd = cmd_args(compile_ctx.clippy_wrapper, clippy_lints, rustc_cmd, clippy_emit_args),
            env = clippy_env,
            diag = "clippy",
            required_outputs = [clippy_out],
            short_cmd = common_args.short_cmd,
            is_binary = False,
            allow_cache_upload = False,
            crate_map = common_args.crate_map,
        )
        diag.update(clippy_diag)

    if toolchain_info.failure_filter:
        # This is only needed when this action's output is being used as an
        # input, so we only need standard diagnostics (clippy is always
        # asked for explicitly).
        stderr = diag["diag.txt"]
        filter_prov = RustFailureFilter(
            buildstatus = build_status,
            required = emit_output,
            stderr = stderr,
        )

        filtered_output = failure_filter(
            ctx = ctx,
            compile_ctx = compile_ctx,
            prefix = "{}/{}".format(common_args.subdir, emit.value),
            predecl_out = predeclared_outputs.get(emit),
            failprov = filter_prov,
            short_cmd = common_args.short_cmd,
        )
    else:
        filtered_output = emit_output

    split_debug_mode = compile_ctx.cxx_toolchain_info.split_debug_mode or SplitDebugMode("none")
    if emit == Emit("link") and split_debug_mode != SplitDebugMode("none"):
        dwo_output_directory = extra_out
        external_debug_info = inherited_external_debug_info(
            ctx = ctx,
            dwo_output_directory = dwo_output_directory,
            dep_link_style = params.dep_link_style,
        )
        dwp_inputs.extend(project_artifacts(ctx.actions, [external_debug_info]))
    else:
        dwo_output_directory = None

    if is_binary and dwp_available(compile_ctx.cxx_toolchain_info):
        dwp_output = dwp(
            ctx,
            compile_ctx.cxx_toolchain_info,
            emit_output,
            identifier = "{}/__{}_{}_dwp".format(common_args.subdir, common_args.tempfile, str(emit)),
            category_suffix = "rust",
            # TODO(T110378142): Ideally, referenced objects are a list of
            # artifacts, but currently we don't track them properly.  So, we
            # just pass in the full link line and extract all inputs from that,
            # which is a bit of an overspecification.
            referenced_objects = dwp_inputs,
        )
    else:
        dwp_output = None

    return RustcOutput(
        output = filtered_output,
        diag = diag,
        pdb = pdb_artifact,
        dwp_output = dwp_output,
        dwo_output_directory = dwo_output_directory,
    )

# --extern <crate>=<path> for direct dependencies
# -Ldependency=<dir> for transitive dependencies
# For native dependencies, we use -Clink-arg=@argsfile
# Second element of result tuple is a list of files/directories that should be present for executable to be run successfully
# Third return is the mapping from crate names back to targets (needed so that a deps linter knows what deps need fixing)
def _dependency_args(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        subdir: str,
        crate_type: CrateType,
        dep_link_style: LinkStyle,
        is_check: bool,
        is_rustdoc_test: bool) -> (cmd_args, list[(CrateName, Label)]):
    args = cmd_args()
    transitive_deps = {}
    crate_targets = []
    available_proc_macros = get_available_proc_macros(ctx)
    for dep in resolve_rust_deps(ctx, include_doc_deps = is_rustdoc_test):
        if dep.name:
            crate = CrateName(
                simple = normalize_crate(dep.name),
                dynamic = None,
            )
        else:
            crate = dep.info.crate

        style = style_info(dep.info, dep_link_style)

        use_rmeta = is_check or (compile_ctx.toolchain_info.pipelined and not crate_type_codegen(crate_type) and not is_rustdoc_test)

        # Use rmeta dependencies whenever possible because they
        # should be cheaper to produce.
        if use_rmeta:
            artifact = style.rmeta
            transitive_artifacts = style.transitive_rmeta_deps
        else:
            artifact = style.rlib
            transitive_artifacts = style.transitive_deps

        for marker in style.transitive_proc_macro_deps.keys():
            info = available_proc_macros[marker.label][RustLinkInfo]
            style = style_info(info, dep_link_style)
            transitive_deps[style.rmeta if use_rmeta else style.rlib] = info.crate

        args.add(extern_arg(ctx, compile_ctx, dep.flags, crate, artifact))
        crate_targets.append((crate, dep.label))

        # Because deps of this *target* can also be transitive deps of this compiler
        # invocation, pass the artifact through `-L` unconditionally for doc tests.
        if is_rustdoc_test:
            transitive_deps[artifact] = crate

        # Unwanted transitive_deps have already been excluded
        transitive_deps.update(transitive_artifacts)

    dynamic_artifacts = {}
    simple_artifacts = {}
    for artifact, crate_name in transitive_deps.items():
        if crate_name.dynamic:
            dynamic_artifacts[artifact] = crate_name
        else:
            simple_artifacts[artifact] = None

    prefix = "{}-deps{}".format(subdir, "-check" if is_check else "")
    if simple_artifacts:
        args.add(simple_symlinked_dirs(ctx, prefix, simple_artifacts))
    if dynamic_artifacts:
        args.add(dynamic_symlinked_dirs(ctx, compile_ctx, prefix, dynamic_artifacts))

    return (args, crate_targets)

def simple_symlinked_dirs(
        ctx: AnalysisContext,
        prefix: str,
        artifacts: dict[Artifact, None]) -> cmd_args:
    # Add as many -Ldependency dirs as we need to avoid name conflicts
    deps_dirs = [{}]
    for dep in artifacts.keys():
        name = dep.basename
        if name in deps_dirs[-1]:
            deps_dirs.append({})
        deps_dirs[-1][name] = dep

    symlinked_dirs = []
    for idx, srcs in enumerate(deps_dirs):
        name = "{}-{}".format(prefix, idx)
        symlinked_dirs.append(ctx.actions.symlinked_dir(name, srcs))

    return cmd_args(symlinked_dirs, format = "-Ldependency={}")

def dynamic_symlinked_dirs(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        prefix: str,
        artifacts: dict[Artifact, CrateName]) -> cmd_args:
    name = "{}-dyn".format(prefix)
    transitive_dependency_dir = ctx.actions.declare_output(name, dir = True)
    do_symlinks = cmd_args(
        compile_ctx.toolchain_info.transitive_dependency_symlinks_tool,
        cmd_args(transitive_dependency_dir.as_output(), format = "--out-dir={}"),
    )
    for artifact, crate in artifacts.items():
        relative_path = cmd_args(artifact).relative_to(transitive_dependency_dir.project("i"))
        do_symlinks.add("--artifact", crate.dynamic, relative_path.ignore_artifacts())
    ctx.actions.run(
        do_symlinks,
        category = "tdep_symlinks",
        identifier = str(len(compile_ctx.transitive_dependency_dirs)),
    )
    compile_ctx.transitive_dependency_dirs[transitive_dependency_dir] = None
    return cmd_args(transitive_dependency_dir, format = "@{}/dirs").hidden(artifacts.keys())

def _lintify(flag: str, clippy: bool, lints: list[ResolvedStringWithMacros]) -> cmd_args:
    return cmd_args(
        [lint for lint in lints if str(lint).startswith("\"clippy::") == clippy],
        format = "-{}{{}}".format(flag),
    )

def _lint_flags(compile_ctx: CompileContext) -> (cmd_args, cmd_args):
    toolchain_info = compile_ctx.toolchain_info

    plain = cmd_args(
        _lintify("A", False, toolchain_info.allow_lints),
        _lintify("D", False, toolchain_info.deny_lints),
        _lintify("W", False, toolchain_info.warn_lints),
    )

    clippy = cmd_args(
        _lintify("A", True, toolchain_info.allow_lints),
        _lintify("D", True, toolchain_info.deny_lints),
        _lintify("W", True, toolchain_info.warn_lints),
    )

    return (plain, clippy)

def _rustc_flags(flags: list[[str, ResolvedStringWithMacros]]) -> list[[str, ResolvedStringWithMacros]]:
    # Rustc's "-g" flag is documented as being exactly equivalent to
    # "-Cdebuginfo=2". Rustdoc supports the latter, it just doesn't have the
    # "-g" shorthand for it.
    for i, flag in enumerate(flags):
        if str(flag) == '"-g"':
            flags[i] = "-Cdebuginfo=2"

    return flags

# Compute which are common to both rustc and rustdoc
def _compute_common_args(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        emit: Emit,
        params: BuildParams,
        dep_link_style: LinkStyle,
        default_roots: list[str],
        is_rustdoc_test: bool) -> CommonArgsInfo:
    exec_is_windows = ctx.attrs._exec_os_type[OsLookup].platform == "windows"
    path_sep = "\\" if exec_is_windows else "/"

    crate_type = params.crate_type

    args_key = (crate_type, emit, dep_link_style, is_rustdoc_test)
    if False:
        # TODO(nga): following `if args_key in ...` is no-op, and typechecker does not like it.
        def unknown():
            pass

        args_key = unknown()
    if args_key in compile_ctx.common_args:
        return compile_ctx.common_args[args_key]

    # Keep filenames distinct in per-flavour subdirs
    subdir = "{}-{}-{}-{}".format(crate_type.value, params.reloc_model.value, dep_link_style.value, emit.value)
    if is_rustdoc_test:
        subdir = "{}-rustdoc-test".format(subdir)

    # Included in tempfiles
    tempfile = "{}-{}".format(attr_simple_crate_for_filenames(ctx), emit.value)

    srcs = ctx.attrs.srcs
    mapped_srcs = ctx.attrs.mapped_srcs
    all_srcs = map(lambda s: s.short_path, srcs) + mapped_srcs.values()
    crate_root = ctx.attrs.crate_root or _crate_root(ctx, all_srcs, default_roots)
    if exec_is_windows:
        crate_root = crate_root.replace("/", "\\")

    is_check = not emit_needs_codegen(emit)

    dependency_args, crate_map = _dependency_args(
        ctx = ctx,
        compile_ctx = compile_ctx,
        subdir = subdir,
        crate_type = crate_type,
        dep_link_style = dep_link_style,
        is_check = is_check,
        is_rustdoc_test = is_rustdoc_test,
    )

    if crate_type == CrateType("proc-macro"):
        dependency_args.add("--extern=proc_macro")

    if crate_type == CrateType("cdylib") and not is_check:
        linker_info = compile_ctx.cxx_toolchain_info.linker_info
        shlib_name = get_default_shared_library_name(linker_info, ctx.label)
        dependency_args.add(cmd_args(
            get_shared_library_name_linker_flags(linker_info.type, shlib_name),
            format = "-Clink-arg={}",
        ))

    toolchain_info = compile_ctx.toolchain_info
    edition = ctx.attrs.edition or toolchain_info.default_edition or \
              fail("missing 'edition' attribute, and there is no 'default_edition' set by the toolchain")

    crate = attr_crate(ctx)
    if crate.dynamic:
        crate_name_arg = cmd_args("--crate-name", cmd_args("@", crate.dynamic, delimiter = ""))
    else:
        crate_name_arg = cmd_args("--crate-name=", crate.simple, delimiter = "")

    split_debuginfo_flags = {
        # Rustc's default behavior: debug info is put into every rlib and
        # staticlib, then copied into the executables and shared libraries by
        # the linker. This corresponds to `-gno-split-dwarf` in Clang.
        SplitDebugMode("none"): [],

        # Split DWARF: debug info is placed into *.dwo files in the directory
        # specified by `--out-dir`. In Buck, this directory is usually called
        # "extras" that is a sibling of the main output artifact (rlib,
        # staticlib, executable, or shared library).
        #
        # Rustc produces one *.dwo per LLVM codegen unit, meaning potentially
        # multiple per crate. The only debug info included into the main output
        # artifact is the list of the associated *.dwo filenames in which the
        # real debug info is provided.
        #
        # For each binary target, we have a separate step which involves
        # `llvm-dwp` to combine all the *.dwo files from the dependency graph
        # into one *.dwp. This is handled as a separate Buck action from the
        # compiler/linker invocation responsible for linking the executable or
        # shared library artifact.
        #
        # Rust's `-Csplit-debuginfo=unpacked` corresponds to `-gsplit-dwarf=split`
        # in Clang, which behaves as just described.
        #
        # There is a second Clang debug mode, `-gsplit-dwarf=single`, that we
        # also implement using `-Csplit-debuginfo=unpacked` in Rust. In Clang,
        # "single" means include debug info into object files conceptually like
        # `-gno-split-dwarf`, but do _not_ copy it at link time into executables
        # and shared libraries. Similar to "split", there is an `llvm-dwp` step,
        # separate from linking, to combine debug info from the dependency graph
        # into one *.dwp output. Rustc has an option `-Csplit-debuginfo=packed`
        # which works this way, putting split debug info into rlibs for
        # libraries but keeping it separate for binaries. We have not been able
        # to use `-Csplit-debuginfo=packed` because it runs into an error "unit
        # referenced by executable was not found" when dealing with chains of
        # dependencies from Rust -> C++ -> Rust (T147665047).
        SplitDebugMode("single"): ["-Csplit-debuginfo=unpacked"],

        # TODO: SplitDebugMode("split"): ["-Csplit-debuginfo=unpacked"],
    }[compile_ctx.cxx_toolchain_info.split_debug_mode or SplitDebugMode("none")]

    args = cmd_args(
        cmd_args(compile_ctx.symlinked_srcs, path_sep, crate_root, delimiter = ""),
        crate_name_arg,
        "--crate-type={}".format(crate_type.value),
        "-Crelocation-model={}".format(params.reloc_model.value),
        "--edition={}".format(edition),
        "-Cmetadata={}".format(_metadata(ctx.label)[0]),
        # Make diagnostics json with the option to extract rendered text
        ["--error-format=json", "--json=diagnostic-rendered-ansi"] if not is_rustdoc_test else [],
        ["-Cprefer-dynamic=yes"] if crate_type == CrateType("dylib") else [],
        ["--target={}".format(toolchain_info.rustc_target_triple)] if toolchain_info.rustc_target_triple else [],
        split_debuginfo_flags,
        _rustc_flags(toolchain_info.rustc_flags),
        _rustc_flags(toolchain_info.rustc_check_flags) if is_check else [],
        _rustc_flags(toolchain_info.rustc_coverage_flags) if ctx.attrs.coverage else [],
        _rustc_flags(ctx.attrs.rustc_flags),
        cmd_args(ctx.attrs.features, format = '--cfg=feature="{}"'),
        dependency_args,
    )

    common_args = CommonArgsInfo(
        args = args,
        subdir = subdir,
        tempfile = tempfile,
        short_cmd = "{},{},{}".format(crate_type.value, params.reloc_model.value, emit.value),
        is_check = is_check,
        crate_map = crate_map,
    )

    compile_ctx.common_args[args_key] = common_args
    return common_args

# Return wrapper script for clippy-driver to make sure sysroot is set right
# We need to make sure clippy is using the same sysroot - compiler, std libraries -
# as rustc itself, so explicitly invoke rustc to get the path. This is a
# (small - ~15ms per invocation) perf hit but only applies when generating
# specifically requested clippy diagnostics.
def _clippy_wrapper(
        ctx: AnalysisContext,
        toolchain_info: RustToolchainInfo) -> cmd_args:
    clippy_driver = cmd_args(toolchain_info.clippy_driver)
    rustc_print_sysroot = cmd_args(toolchain_info.compiler, "--print=sysroot", delimiter = " ")
    if toolchain_info.rustc_target_triple:
        rustc_print_sysroot.add("--target={}".format(toolchain_info.rustc_target_triple))

    if ctx.attrs._exec_os_type[OsLookup].platform == "windows":
        wrapper_file, _ = ctx.actions.write(
            ctx.actions.declare_output("__clippy_driver_wrapper.bat"),
            [
                "@echo off",
                "set __CLIPPY_INTERNAL_TESTS=true",
                cmd_args(rustc_print_sysroot, format = 'FOR /F "tokens=* USEBACKQ" %%F IN (`{}`) DO ('),
                "set SYSROOT=%%F",
                ")",
                cmd_args(clippy_driver, format = "{} %*"),
            ],
            allow_args = True,
        )
    else:
        wrapper_file, _ = ctx.actions.write(
            ctx.actions.declare_output("__clippy_driver_wrapper.sh"),
            [
                "#!/usr/bin/env bash",
                # Force clippy to be clippy: https://github.com/rust-lang/rust-clippy/blob/e405c68b3c1265daa9a091ed9b4b5c5a38c0c0ba/src/driver.rs#L334
                "export __CLIPPY_INTERNAL_TESTS=true",
                cmd_args(rustc_print_sysroot, format = "export SYSROOT=$({})"),
                cmd_args(clippy_driver, format = "{} \"$@\"\n"),
            ],
            is_executable = True,
            allow_args = True,
        )

    return cmd_args(wrapper_file).hidden(clippy_driver, rustc_print_sysroot)

# This is a hack because we need to pass the linker to rustc
# using -Clinker=path and there is currently no way of doing this
# without an artifact. We create a wrapper (which is an artifact),
# and add -Clinker=
def _linker_args(
        ctx: AnalysisContext,
        linker_info: LinkerInfo) -> cmd_args:
    linker = cmd_args(
        linker_info.linker,
        linker_info.linker_flags or [],
        ctx.attrs.linker_flags,
    )

    linker_wrapper = cmd_script(
        ctx = ctx,
        name = "linker_wrapper",
        cmd = linker,
        os = ScriptOs("windows" if ctx.attrs._exec_os_type[OsLookup].platform == "windows" else "unix"),
    )

    return cmd_args(linker_wrapper, format = "-Clinker={}")

# Returns the full label and its hash. The full label is used for `-Cmetadata`
# which provided the primary disambiguator for two otherwise identically named
# crates. The hash is added to the filename to give them a lower likelihood of
# duplicate names, but it doesn't matter if they collide.
def _metadata(label: Label) -> (str, str):
    label = str(label.raw_target())
    h = hash(label)
    if h < 0:
        h = -h
    h = "%x" % h
    return (label, "0" * (8 - len(h)) + h)

def _crate_root(
        ctx: AnalysisContext,
        srcs: list[str],
        default_roots: list[str]) -> str:
    candidates = set()
    if getattr(ctx.attrs, "crate_dynamic", None):
        crate_with_suffix = None
    else:
        crate_with_suffix = attr_crate(ctx).simple + ".rs"
    for src in srcs:
        filename = src.split("/")[-1]
        if filename in default_roots or filename == crate_with_suffix:
            candidates.add(src)

    if candidates.size() == 1:
        return candidates.list()[0]

    fail("Could not infer crate_root. candidates=%s\nAdd 'crate_root = \"src/example.rs\"' to your attributes to disambiguate." % candidates.list())

# Take a desired output and work out how to convince rustc to generate it
def _rustc_emit(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        emit: Emit,
        predeclared_outputs: dict[Emit, Artifact],
        subdir: str,
        params: BuildParams) -> (Artifact, cmd_args, [Artifact, None]):
    toolchain_info = compile_ctx.toolchain_info
    simple_crate = attr_simple_crate_for_filenames(ctx)
    crate_type = params.crate_type

    # Metadata for pipelining needs has enough info to be used as an input for
    # dependents. To do this reliably, follow Cargo's pattern of always doing
    # --emit metadata,link, but only using the output we actually need.
    #
    # We don't bother to do this with "codegen" crates - ie, ones which are
    # linked into an artifact like binaries and dylib, since they're not used as
    # a pipelined dependency input.
    pipeline_artifact = toolchain_info.pipelined and \
                        emit in (Emit("metadata"), Emit("link")) and \
                        not crate_type_codegen(crate_type)

    emit_args = cmd_args()
    if emit in predeclared_outputs:
        emit_output = predeclared_outputs[emit]
    else:
        extra_hash = "-" + _metadata(ctx.label)[1]
        emit_args.add("-Cextra-filename={}".format(extra_hash))
        filename = subdir + "/" + output_filename(simple_crate, emit, params, extra_hash)

        emit_output = ctx.actions.declare_output(filename)

    # For pipelined builds if we're emitting either metadata or link then make
    # sure we generate both and take the one we want.
    if pipeline_artifact:
        metaext = "" if emit == Emit("metadata") else "_unwanted"
        linkext = "" if emit == Emit("link") else "_unwanted"

        emit_args.add(
            cmd_args("--emit=metadata=", emit_output.as_output(), metaext, delimiter = ""),
            cmd_args("--emit=link=", emit_output.as_output(), linkext, delimiter = ""),
        )
    elif emit == Emit("expand"):
        emit_args.add(
            "-Zunpretty=expanded",
            cmd_args(emit_output.as_output(), format = "-o{}"),
        )
    else:
        # Assume https://github.com/rust-lang/rust/issues/85356 is fixed (ie
        # https://github.com/rust-lang/rust/pull/85362 is applied)
        emit_args.add(cmd_args("--emit=", emit.value, "=", emit_output.as_output(), delimiter = ""))

    extra_out = None
    if emit != Emit("expand"):
        # Strip file extension from directory name.
        base, _ext = paths.split_extension(output_filename(simple_crate, emit, params))
        extra_dir = subdir + "/extras/" + base
        extra_out = ctx.actions.declare_output(extra_dir, dir = True)
        emit_args.add(cmd_args(extra_out.as_output(), format = "--out-dir={}"))

        if ctx.attrs.incremental_enabled:
            build_mode = ctx.attrs.incremental_build_mode
            incremental_out = ctx.actions.declare_output("{}/extras/incremental/{}".format(subdir, build_mode))
            incremental_cmd = cmd_args(incremental_out.as_output(), format = "-Cincremental={}")
            emit_args.add(incremental_cmd)

    return (emit_output, emit_args, extra_out)

# Invoke rustc and capture outputs
def _rustc_invoke(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        prefix: str,
        rustc_cmd: cmd_args,
        diag: str,
        required_outputs: list[Artifact],
        short_cmd: str,
        is_binary: bool,
        allow_cache_upload: bool,
        crate_map: list[(CrateName, Label)],
        env: dict[str, [ResolvedStringWithMacros, Artifact]] = {},
        only_artifact: [None, str] = None) -> (dict[str, Artifact], [Artifact, None]):
    exec_is_windows = ctx.attrs._exec_os_type[OsLookup].platform == "windows"

    toolchain_info = compile_ctx.toolchain_info

    plain_env, path_env = _process_env(compile_ctx, ctx.attrs.env, exec_is_windows)

    more_plain_env, more_path_env = _process_env(compile_ctx, env, exec_is_windows)
    plain_env.update(more_plain_env)
    path_env.update(more_path_env)

    # Save diagnostic outputs
    json_diag = ctx.actions.declare_output("{}-{}.json".format(prefix, diag))
    txt_diag = ctx.actions.declare_output("{}-{}.txt".format(prefix, diag))

    compile_cmd = cmd_args(
        cmd_args(json_diag.as_output(), format = "--diag-json={}"),
        cmd_args(txt_diag.as_output(), format = "--diag-txt={}"),
        "--remap-cwd-prefix=.",
        "--buck-target={}".format(ctx.label.raw_target()),
    )

    if only_artifact:
        compile_cmd.add("--only-artifact=" + only_artifact)

    for k, v in crate_map:
        compile_cmd.add(crate_map_arg(ctx, compile_ctx, k, v))
    for k, v in plain_env.items():
        compile_cmd.add(cmd_args("--env=", k, "=", v, delimiter = ""))
    for k, v in path_env.items():
        compile_cmd.add(cmd_args("--path-env=", k, "=", v, delimiter = ""))

    build_status = None
    if toolchain_info.failure_filter:
        # Build status for fail filter
        build_status = ctx.actions.declare_output("{}_build_status-{}.json".format(prefix, diag))
        compile_cmd.add(cmd_args(build_status.as_output(), format = "--failure-filter={}"))
        for out in required_outputs:
            compile_cmd.add("--required-output", out.short_path, out.as_output())

    compile_cmd.add(rustc_cmd)
    compile_cmd.hidden(toolchain_info.compiler, compile_ctx.symlinked_srcs)

    compile_cmd = _long_command(
        ctx = ctx,
        exe = toolchain_info.rustc_action,
        args = compile_cmd,
        argfile_name = "{}-{}.args".format(prefix, diag),
    )

    incremental_enabled = ctx.attrs.incremental_enabled
    local_only = False
    prefer_local = False
    if incremental_enabled:
        local_only = True
    elif is_binary and link_cxx_binary_locally(ctx):
        prefer_local = True

    identifier = "{} {} [{}]".format(prefix, short_cmd, diag)
    ctx.actions.run(
        compile_cmd,
        local_only = local_only,
        prefer_local = prefer_local,
        category = "rustc",
        identifier = identifier,
        no_outputs_cleanup = incremental_enabled,
        allow_cache_upload = allow_cache_upload,
    )

    return ({diag + ".json": json_diag, diag + ".txt": txt_diag}, build_status)

# Our rustc and rustdoc commands can have arbitrarily large number of `--extern`
# flags, so write to file to avoid hitting the platform's limit on command line
# length. This limit is particularly small on Windows.
def _long_command(
        ctx: AnalysisContext,
        exe: RunInfo,
        args: cmd_args,
        argfile_name: str) -> cmd_args:
    argfile, hidden = ctx.actions.write(argfile_name, args, allow_args = True)
    return cmd_args(exe, cmd_args(argfile, format = "@{}")).hidden(args, hidden)

_DOUBLE_ESCAPED_NEWLINE_RE = regex("\\\\n")
_ESCAPED_NEWLINE_RE = regex("\\n")

# Separate env settings into "plain" and "with path". Path env vars are often
# used in Rust `include!()` and similar directives, which always interpret the
# path relative to the source file containing the directive. Since paths in env
# vars are often expanded from macros such as `$(location)`, they will be
# cell-relative which will not work properly. To solve this, we canonicalize
# paths to absolute paths so they'll work in any context. Hence the need to
# distinguish path from non-path. (This will not work if the value contains both
# path and non-path content, but we'll burn that bridge when we get to it.)
def _process_env(
        compile_ctx: CompileContext,
        env: dict[str, [ResolvedStringWithMacros, Artifact]],
        exec_is_windows: bool) -> (dict[str, cmd_args], dict[str, cmd_args]):
    # Values with inputs (ie artifact references).
    path_env = {}

    # Plain strings.
    plain_env = {}

    for k, v in env.items():
        v = cmd_args(v)
        if len(v.inputs) > 0:
            path_env[k] = v
        else:
            # Environment variables may have newlines, escape them for now.
            # Will be unescaped in rustc_action.
            # Variable may have "\\n" as well.
            # Example: \\n\n -> \\\n\n -> \\\\n\\n
            plain_env[k] = v.replace_regex(_DOUBLE_ESCAPED_NEWLINE_RE, "\\\n").replace_regex(_ESCAPED_NEWLINE_RE, "\\n")

    # If CARGO_MANIFEST_DIR is not already expressed in terms of $(location ...)
    # of some target, then interpret it as a relative path inside of the crate's
    # sources.
    #
    # For example in the following case:
    #
    #     http_archive(
    #         name = "foo.crate",
    #         ...
    #     )
    #
    #     rust_library(
    #         name = "foo",
    #         srcs = [":foo.crate"],
    #         crate_root = "foo.crate/src/lib.rs",
    #         env = {
    #             "CARGO_MANIFEST_DIR": "foo.crate",
    #         },
    #     )
    #
    # then the manifest directory refers to the directory which is the parent of
    # `src` inside the archive.
    #
    # By putting the environment variable into path_env, rustc_action.py will
    # take care of turning this into an absolute path before rustc sees it. This
    # matches Cargo which also always provides CARGO_MANIFEST_DIR as an absolute
    # path. A relative path would be problematic because it can't simultaneously
    # support both of the following real-world cases: `include!` which resolves
    # relative paths relative to the file containing the include:
    #
    #     include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/thing.rs"));
    #
    # and proc macros using std::fs to read thing like .pest grammars, which
    # would need paths relative to the directory that rustc got invoked in
    # (which is the repo root in Buck builds).
    cargo_manifest_dir = plain_env.pop("CARGO_MANIFEST_DIR", None)
    if cargo_manifest_dir:
        path_env["CARGO_MANIFEST_DIR"] = cmd_args(
            compile_ctx.symlinked_srcs,
            "\\" if exec_is_windows else "/",
            cargo_manifest_dir,
            delimiter = "",
        )

    return (plain_env, path_env)

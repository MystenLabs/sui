# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//:artifact_tset.bzl",
    "ArtifactTSet",
    "make_artifact_tset",
)
load("@prelude//:paths.bzl", "paths")
load("@prelude//:resources.bzl", "ResourceInfo", "gather_resources")
load(
    "@prelude//android:android_providers.bzl",
    "merge_android_packageable_info",
)
load(
    "@prelude//cxx:cxx_context.bzl",
    "get_cxx_toolchain_info",
)
load("@prelude//cxx:cxx_toolchain_types.bzl", "PicBehavior")
load(
    "@prelude//cxx:linker.bzl",
    "PDB_SUB_TARGET",
    "get_default_shared_library_name",
    "get_pdb_providers",
)
load(
    "@prelude//cxx:omnibus.bzl",
    "create_linkable_root",
)
load(
    "@prelude//linking:link_groups.bzl",
    "merge_link_group_lib_info",
)
load(
    "@prelude//linking:link_info.bzl",
    "Archive",
    "ArchiveLinkable",
    "LibOutputStyle",
    "LinkInfo",
    "LinkInfos",
    "LinkStrategy",
    "LinkStyle",
    "Linkage",
    "LinkedObject",
    "MergedLinkInfo",
    "SharedLibLinkable",
    "create_merged_link_info",
    "create_merged_link_info_for_propagation",
    "get_lib_output_style",
    "legacy_output_style_to_link_style",
)
load(
    "@prelude//linking:linkable_graph.bzl",
    "DlopenableLibraryInfo",
    "create_linkable_graph",
    "create_linkable_graph_node",
    "create_linkable_node",
)
load(
    "@prelude//linking:shared_libraries.bzl",
    "create_shared_libraries",
    "merge_shared_libraries",
)
load("@prelude//linking:strip.bzl", "strip_debug_info")
load("@prelude//os_lookup:defs.bzl", "OsLookup")
load(
    ":build.bzl",
    "RustcOutput",  # @unused Used as a type
    "compile_context",
    "generate_rustdoc",
    "generate_rustdoc_test",
    "rust_compile",
    "rust_compile_multi",
)
load(
    ":build_params.bzl",
    "BuildParams",  # @unused Used as a type
    "Emit",
    "LinkageLang",
    "RuleType",
    "build_params",
    "crate_type_transitive_deps",
)
load(
    ":context.bzl",
    "CompileContext",  # @unused Used as a type
)
load(
    ":link_info.bzl",
    "CrateName",  # @unused Used as a type
    "DEFAULT_STATIC_LINK_STYLE",
    "RustLinkInfo",
    "RustLinkStyleInfo",
    "RustProcMacroMarker",  # @unused Used as a type
    "attr_crate",
    "inherited_exported_link_deps",
    "inherited_external_debug_info",
    "inherited_merged_link_infos",
    "inherited_shared_libs",
    "resolve_deps",
    "resolve_rust_deps",
    "style_info",
)
load(":proc_macro_alias.bzl", "rust_proc_macro_alias")
load(":resources.bzl", "rust_attr_resources")
load(":targets.bzl", "targets")

def prebuilt_rust_library_impl(ctx: AnalysisContext) -> list[Provider]:
    providers = []

    # Default output.
    providers.append(
        DefaultInfo(
            default_output = ctx.attrs.rlib,
        ),
    )

    # Rust link provider.
    crate = attr_crate(ctx)
    styles = {}
    for style in LinkStyle:
        dep_link_style = style
        tdeps, tmetadeps, external_debug_info, tprocmacrodeps = _compute_transitive_deps(ctx, dep_link_style)
        external_debug_info = make_artifact_tset(
            actions = ctx.actions,
            children = external_debug_info,
        )
        styles[style] = RustLinkStyleInfo(
            rlib = ctx.attrs.rlib,
            transitive_deps = tdeps,
            rmeta = ctx.attrs.rlib,
            transitive_rmeta_deps = tmetadeps,
            transitive_proc_macro_deps = tprocmacrodeps,
            pdb = None,
            external_debug_info = external_debug_info,
        )

    # Prebuilt libraries only work in unbundled mode, as they only support `rlib`
    # files today.
    native_unbundle_deps = True
    providers.append(
        RustLinkInfo(
            crate = crate,
            styles = styles,
            exported_link_deps = inherited_exported_link_deps(ctx, native_unbundle_deps),
            merged_link_info = create_merged_link_info_for_propagation(ctx, inherited_merged_link_infos(ctx, native_unbundle_deps)),
            shared_libs = merge_shared_libraries(
                ctx.actions,
                deps = inherited_shared_libs(ctx, native_unbundle_deps),
            ),
        ),
    )

    # Native link provier.
    link = LinkInfos(
        default = LinkInfo(
            linkables = [
                ArchiveLinkable(
                    archive = Archive(artifact = ctx.attrs.rlib),
                    linker_type = "unknown",
                ),
            ],
        ),
        stripped = LinkInfo(
            linkables = [
                ArchiveLinkable(
                    archive = Archive(
                        artifact = strip_debug_info(
                            ctx = ctx,
                            out = ctx.attrs.rlib.short_path,
                            obj = ctx.attrs.rlib,
                        ),
                    ),
                    linker_type = "unknown",
                ),
            ],
        ),
    )
    providers.append(
        create_merged_link_info(
            ctx,
            PicBehavior("supported"),
            {output_style: link for output_style in LibOutputStyle},
            exported_deps = [d[MergedLinkInfo] for d in ctx.attrs.deps],
            # TODO(agallagher): This matches v1 behavior, but some of these libs
            # have prebuilt DSOs which might be usable.
            preferred_linkage = Linkage("static"),
        ),
    )

    # Native link graph setup.
    linker_info = get_cxx_toolchain_info(ctx).linker_info
    linkable_graph = create_linkable_graph(
        ctx,
        node = create_linkable_graph_node(
            ctx,
            linkable_node = create_linkable_node(
                ctx = ctx,
                preferred_linkage = Linkage("static"),
                exported_deps = ctx.attrs.deps,
                link_infos = {output_style: link for output_style in LibOutputStyle},
                default_soname = get_default_shared_library_name(linker_info, ctx.label),
            ),
        ),
        deps = ctx.attrs.deps,
    )
    providers.append(linkable_graph)

    providers.append(merge_link_group_lib_info(deps = ctx.attrs.deps))

    providers.append(merge_android_packageable_info(ctx.label, ctx.actions, ctx.attrs.deps))

    return providers

def rust_library_impl(ctx: AnalysisContext) -> list[Provider]:
    compile_ctx = compile_context(ctx)
    toolchain_info = compile_ctx.toolchain_info

    # Multiple styles and language linkages could generate the same crate types
    # (eg procmacro or using preferred_linkage), so we need to see how many
    # distinct kinds of build we actually need to deal with.
    param_lang, lang_style_param = _build_params_for_styles(ctx, compile_ctx)

    artifacts = _build_library_artifacts(ctx, compile_ctx, param_lang.keys())

    rust_param_artifact = {}
    native_param_artifact = {}
    check_artifacts = None

    for params, (link, meta) in artifacts.items():
        if LinkageLang("rust") in param_lang[params]:
            # Grab the check output for all kinds of builds to use
            # in the check subtarget. The link style doesn't matter
            # so pick the first.
            if check_artifacts == None:
                check_artifacts = {"check": meta.output}
                check_artifacts.update(meta.diag)

            rust_param_artifact[params] = _handle_rust_artifact(ctx, params, link, meta)
        if LinkageLang("native") in param_lang[params] or LinkageLang("native-unbundled") in param_lang[params]:
            native_param_artifact[params] = link

    # Among {rustdoc, doctests, macro expand}, doctests are the only one which
    # cares about linkage. So if there is a required link style set for the
    # doctests, reuse those same dependency artifacts for the other build
    # outputs where static vs static_pic does not make a difference.
    if ctx.attrs.doc_link_style:
        static_link_style = {
            "shared": DEFAULT_STATIC_LINK_STYLE,
            "static": LinkStyle("static"),
            "static_pic": LinkStyle("static_pic"),
        }[ctx.attrs.doc_link_style]
    else:
        static_link_style = DEFAULT_STATIC_LINK_STYLE

    static_library_params = lang_style_param[(LinkageLang("rust"), static_link_style)]
    default_roots = ["lib.rs"]
    rustdoc = generate_rustdoc(
        ctx = ctx,
        compile_ctx = compile_ctx,
        params = static_library_params,
        default_roots = default_roots,
        document_private_items = False,
    )

    # If doctests=True or False is set on the individual target, respect that.
    # Otherwise look at the global setting on the toolchain.
    doctests_enabled = ctx.attrs.doctests if ctx.attrs.doctests != None else toolchain_info.doctests

    rustdoc_test = None
    if doctests_enabled and toolchain_info.rustc_target_triple == targets.exec_triple(ctx):
        if ctx.attrs.doc_link_style:
            doc_link_style = LinkStyle(ctx.attrs.doc_link_style)
        else:
            doc_link_style = {
                "any": LinkStyle("shared"),
                "shared": LinkStyle("shared"),
                "static": DEFAULT_STATIC_LINK_STYLE,
            }[ctx.attrs.preferred_linkage]
        rustdoc_test_params = build_params(
            rule = RuleType("binary"),
            proc_macro = ctx.attrs.proc_macro,
            link_style = doc_link_style,
            preferred_linkage = Linkage(ctx.attrs.preferred_linkage),
            lang = LinkageLang("rust"),
            linker_type = compile_ctx.cxx_toolchain_info.linker_info.type,
            target_os_type = ctx.attrs._target_os_type[OsLookup],
        )
        rustdoc_test = generate_rustdoc_test(
            ctx = ctx,
            compile_ctx = compile_ctx,
            link_style = rustdoc_test_params.dep_link_style,
            library = rust_param_artifact[static_library_params],
            params = rustdoc_test_params,
            default_roots = default_roots,
        )

    expand = rust_compile(
        ctx = ctx,
        compile_ctx = compile_ctx,
        emit = Emit("expand"),
        params = static_library_params,
        dep_link_style = DEFAULT_STATIC_LINK_STYLE,
        default_roots = default_roots,
    )

    providers = []

    providers += _default_providers(
        lang_style_param = lang_style_param,
        param_artifact = rust_param_artifact,
        rustdoc = rustdoc,
        rustdoc_test = rustdoc_test,
        check_artifacts = check_artifacts,
        expand = expand.output,
        sources = compile_ctx.symlinked_srcs,
    )
    providers += _rust_providers(
        ctx = ctx,
        compile_ctx = compile_ctx,
        lang_style_param = lang_style_param,
        param_artifact = rust_param_artifact,
    )
    providers += _native_providers(
        ctx = ctx,
        compile_ctx = compile_ctx,
        lang_style_param = lang_style_param,
        param_artifact = native_param_artifact,
    )

    deps = [dep.dep for dep in resolve_deps(ctx)]
    providers.append(ResourceInfo(resources = gather_resources(
        label = ctx.label,
        resources = rust_attr_resources(ctx),
        deps = deps,
    )))

    providers.append(merge_android_packageable_info(ctx.label, ctx.actions, deps))

    return providers

def _build_params_for_styles(
        ctx: AnalysisContext,
        compile_ctx: CompileContext) -> (
    dict[BuildParams, list[LinkageLang]],
    dict[(LinkageLang, LinkStyle), BuildParams],
):
    """
    For a given rule, return two things:
    - a set of build params we need for all combinations of linkage langages and
      link styles, mapped to which languages they apply to
    - a mapping from linkage language and link style to build params

    This is needed because different combinations may end up using the same set
    of params, and we want to minimize invocations to rustc, both for
    efficiency's sake, but also to avoid duplicate objects being linked
    together.
    """

    param_lang = {}  # param -> linkage_lang
    style_param = {}  # (linkage_lang, output_style) -> param

    target_os_type = ctx.attrs._target_os_type[OsLookup]
    linker_type = compile_ctx.cxx_toolchain_info.linker_info.type

    # Styles+lang linkage to params
    for linkage_lang in LinkageLang:
        # Skip proc_macro + non-rust combinations
        if ctx.attrs.proc_macro and linkage_lang != LinkageLang("rust"):
            continue

        for link_style in LinkStyle:
            params = build_params(
                rule = RuleType("library"),
                proc_macro = ctx.attrs.proc_macro,
                link_style = link_style,
                preferred_linkage = Linkage(ctx.attrs.preferred_linkage),
                lang = linkage_lang,
                linker_type = linker_type,
                target_os_type = target_os_type,
            )
            if params not in param_lang:
                param_lang[params] = []
            param_lang[params] = param_lang[params] + [linkage_lang]
            style_param[(linkage_lang, link_style)] = params

    return (param_lang, style_param)

def _build_library_artifacts(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        params: list[BuildParams]) -> dict[BuildParams, (RustcOutput, RustcOutput)]:
    """
    Generate the actual actions to build various output artifacts. Given the set
    parameters we need, return a mapping to the linkable and metadata artifacts.
    """
    param_artifact = {}

    for params in params:
        dep_link_style = params.dep_link_style

        # Separate actions for each emit type
        #
        # In principle we don't really need metadata for C++-only artifacts, but I don't think it hurts
        link, meta = rust_compile_multi(
            ctx = ctx,
            compile_ctx = compile_ctx,
            emits = [Emit("link"), Emit("metadata")],
            params = params,
            dep_link_style = dep_link_style,
            default_roots = ["lib.rs"],
        )

        param_artifact[params] = (link, meta)

    return param_artifact

def _handle_rust_artifact(
        ctx: AnalysisContext,
        params: BuildParams,
        link: RustcOutput,
        meta: RustcOutput) -> RustLinkStyleInfo:
    """
    Return the RustLinkInfo for a given set of artifacts. The main consideration
    is computing the right set of dependencies.
    """

    dep_link_style = params.dep_link_style

    # If we're a crate where our consumers should care about transitive deps,
    # then compute them (specifically, not proc-macro).
    if crate_type_transitive_deps(params.crate_type):
        tdeps, tmetadeps, external_debug_info, tprocmacrodeps = _compute_transitive_deps(ctx, dep_link_style)
    else:
        tdeps, tmetadeps, external_debug_info, tprocmacrodeps = {}, {}, [], {}

    if not ctx.attrs.proc_macro:
        external_debug_info = make_artifact_tset(
            actions = ctx.actions,
            label = ctx.label,
            artifacts = filter(None, [link.dwo_output_directory]),
            children = external_debug_info,
        )
        return RustLinkStyleInfo(
            rlib = link.output,
            transitive_deps = tdeps,
            rmeta = meta.output,
            transitive_rmeta_deps = tmetadeps,
            transitive_proc_macro_deps = tprocmacrodeps,
            pdb = link.pdb,
            external_debug_info = external_debug_info,
        )
    else:
        # Proc macro deps are always the real thing
        return RustLinkStyleInfo(
            rlib = link.output,
            transitive_deps = tdeps,
            rmeta = link.output,
            transitive_rmeta_deps = tdeps,
            transitive_proc_macro_deps = tprocmacrodeps,
            pdb = link.pdb,
            external_debug_info = ArtifactTSet(),
        )

def _default_providers(
        lang_style_param: dict[(LinkageLang, LinkStyle), BuildParams],
        param_artifact: dict[BuildParams, RustLinkStyleInfo],
        rustdoc: Artifact,
        rustdoc_test: [(cmd_args, dict[str, cmd_args]), None],
        check_artifacts: dict[str, Artifact],
        expand: Artifact,
        sources: Artifact) -> list[Provider]:
    targets = {}
    targets.update(check_artifacts)
    targets["sources"] = sources
    targets["expand"] = expand
    targets["doc"] = rustdoc
    sub_targets = {
        k: [DefaultInfo(default_output = v)]
        for (k, v) in targets.items()
    }

    # Add provider for default output, and for each link-style...
    for link_style in LinkStyle:
        link_style_info = param_artifact[lang_style_param[(LinkageLang("rust"), link_style)]]
        nested_sub_targets = {}
        if link_style_info.pdb:
            nested_sub_targets[PDB_SUB_TARGET] = get_pdb_providers(pdb = link_style_info.pdb, binary = link_style_info.rlib)
        sub_targets[link_style.value] = [DefaultInfo(
            default_output = link_style_info.rlib,
            sub_targets = nested_sub_targets,
        )]

    providers = []

    if rustdoc_test:
        (rustdoc_cmd, rustdoc_env) = rustdoc_test
        rustdoc_test_info = ExternalRunnerTestInfo(
            type = "rustdoc",
            command = [rustdoc_cmd],
            run_from_project_root = True,
            env = rustdoc_env,
        )

        # Run doc test as part of `buck2 test :crate`
        providers.append(rustdoc_test_info)

        # Run doc test as part of `buck2 test :crate[doc]`
        sub_targets["doc"].append(rustdoc_test_info)

    providers.append(DefaultInfo(
        default_output = check_artifacts["check"],
        sub_targets = sub_targets,
    ))

    return providers

def _rust_providers(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        lang_style_param: dict[(LinkageLang, LinkStyle), BuildParams],
        param_artifact: dict[BuildParams, RustLinkStyleInfo]) -> list[Provider]:
    """
    Return the set of providers for Rust linkage.
    """
    crate = attr_crate(ctx)
    native_unbundle_deps = compile_ctx.toolchain_info.native_unbundle_deps

    style_info = {
        link_style: param_artifact[lang_style_param[(LinkageLang("rust"), link_style)]]
        for link_style in LinkStyle
    }

    # Inherited link input and shared libraries.  As in v1, this only includes
    # non-Rust rules, found by walking through -- and ignoring -- Rust libraries
    # to find non-Rust native linkables and libraries.
    if not ctx.attrs.proc_macro:
        inherited_link_deps = inherited_exported_link_deps(ctx, native_unbundle_deps)
        inherited_link_infos = inherited_merged_link_infos(ctx, native_unbundle_deps)
        inherited_shlibs = inherited_shared_libs(ctx, native_unbundle_deps)
    else:
        # proc-macros are just used by the compiler and shouldn't propagate
        # their native deps to the link line of the target.
        inherited_link_infos = []
        inherited_shlibs = []
        inherited_link_deps = []

    providers = []

    # Create rust library provider.
    providers.append(RustLinkInfo(
        crate = crate,
        styles = style_info,
        merged_link_info = create_merged_link_info_for_propagation(ctx, inherited_link_infos),
        exported_link_deps = inherited_link_deps,
        shared_libs = merge_shared_libraries(
            ctx.actions,
            deps = inherited_shlibs,
        ),
    ))

    return providers

def _native_providers(
        ctx: AnalysisContext,
        compile_ctx: CompileContext,
        lang_style_param: dict[(LinkageLang, LinkStyle), BuildParams],
        param_artifact: dict[BuildParams, RustcOutput]) -> list[Provider]:
    """
    Return the set of providers needed to link Rust as a dependency for native
    (ie C/C++) code, along with relevant dependencies.
    """

    # If native_unbundle_deps is set on the the rust toolchain, then build this artifact
    # using the "native-unbundled" linkage language. See LinkageLang docs for more details
    native_unbundle_deps = compile_ctx.toolchain_info.native_unbundle_deps
    lang = LinkageLang("native-unbundled") if native_unbundle_deps else LinkageLang("native")

    inherited_link_deps = inherited_exported_link_deps(ctx, native_unbundle_deps)
    inherited_link_infos = inherited_merged_link_infos(ctx, native_unbundle_deps)
    inherited_shlibs = inherited_shared_libs(ctx, native_unbundle_deps)
    linker_info = compile_ctx.cxx_toolchain_info.linker_info
    linker_type = linker_info.type

    providers = []

    if ctx.attrs.proc_macro:
        # Proc-macros never have a native form
        return providers

    # TODO(cjhopman): This seems to be conflating the link strategy with the lib output style. I tried going through
    # lang_style_param/BuildParams and make it actually be based on LibOutputStyle, but it goes on to use that for defining
    # how to consume dependencies and it's used for rust_binary like its own link strategy and it's unclear what's the
    # correct behavior. For now, this preserves existing behavior without clarifying what concepts its actually
    # operating on.
    libraries = {}
    link_infos = {}
    external_debug_infos = {}
    for output_style in LibOutputStyle:
        legacy_link_style = legacy_output_style_to_link_style(output_style)
        params = lang_style_param[(lang, legacy_link_style)]
        lib = param_artifact[params]
        libraries[output_style] = lib

        external_debug_info = inherited_external_debug_info(
            ctx = ctx,
            dwo_output_directory = lib.dwo_output_directory,
            dep_link_style = params.dep_link_style,
        )
        external_debug_infos[output_style] = external_debug_info

        # DO NOT COMMIT: verify this change
        if output_style == LibOutputStyle("shared_lib"):
            link_infos[output_style] = LinkInfos(
                default = LinkInfo(
                    linkables = [SharedLibLinkable(lib = lib.output)],
                    external_debug_info = external_debug_info,
                ),
                stripped = LinkInfo(
                    linkables = [ArchiveLinkable(
                        archive = Archive(
                            artifact = strip_debug_info(
                                ctx,
                                paths.join(output_style.value, lib.output.short_path),
                                lib.output,
                            ),
                        ),
                        linker_type = linker_type,
                    )],
                ),
            )
        else:
            link_infos[output_style] = LinkInfos(
                default = LinkInfo(
                    linkables = [ArchiveLinkable(
                        archive = Archive(artifact = lib.output),
                        linker_type = linker_type,
                    )],
                    external_debug_info = external_debug_info,
                ),
            )

    preferred_linkage = Linkage(ctx.attrs.preferred_linkage)

    # TODO(cjhopman): This is preserving existing behavior, but it doesn't make sense. These lists can be passed
    # unmerged to create_merged_link_info below. Potentially that could change link order, so needs to be done more carefully.
    merged_inherited_link = create_merged_link_info_for_propagation(ctx, inherited_link_infos)

    # Native link provider.
    providers.append(create_merged_link_info(
        ctx,
        compile_ctx.cxx_toolchain_info.pic_behavior,
        link_infos,
        exported_deps = [merged_inherited_link],
        preferred_linkage = preferred_linkage,
    ))

    solibs = {}

    # Add the shared library to the list of shared libs.
    linker_info = compile_ctx.cxx_toolchain_info.linker_info
    shlib_name = get_default_shared_library_name(linker_info, ctx.label)

    # Only add a shared library if we generated one.
    # TODO(cjhopman): This is strange. Normally (like in c++) the link_infos passed to create_merged_link_info above would only have
    # a value for LibOutputStyle("shared_lib") if that were created and we could just check for that key. Given that I intend
    # to remove the SharedLibraries provider, maybe just wait for that to resolve this.
    if get_lib_output_style(LinkStrategy("shared"), preferred_linkage, compile_ctx.cxx_toolchain_info.pic_behavior) == LibOutputStyle("shared_lib"):
        solibs[shlib_name] = LinkedObject(
            output = libraries[LibOutputStyle("shared_lib")].output,
            unstripped_output = libraries[LibOutputStyle("shared_lib")].output,
            external_debug_info = external_debug_infos[LibOutputStyle("shared_lib")],
        )

    # Native shared library provider.
    providers.append(merge_shared_libraries(
        ctx.actions,
        create_shared_libraries(ctx, solibs),
        inherited_shlibs,
    ))

    # Omnibus root provider.
    linkable_root = create_linkable_root(
        name = shlib_name,
        link_infos = LinkInfos(
            default = LinkInfo(
                linkables = [ArchiveLinkable(
                    archive = Archive(
                        artifact = libraries[LibOutputStyle("shared_lib")].output,
                    ),
                    linker_type = linker_type,
                    link_whole = True,
                )],
                external_debug_info = external_debug_infos[LibOutputStyle("pic_archive")],
            ),
        ),
        deps = inherited_link_deps,
    )
    providers.append(linkable_root)

    # Mark libraries that support `dlopen`.
    if getattr(ctx.attrs, "supports_python_dlopen", False):
        providers.append(DlopenableLibraryInfo())

    linkable_graph = create_linkable_graph(
        ctx,
        node = create_linkable_graph_node(
            ctx,
            linkable_node = create_linkable_node(
                ctx = ctx,
                preferred_linkage = preferred_linkage,
                exported_deps = inherited_link_deps,
                link_infos = link_infos,
                shared_libs = solibs,
                default_soname = shlib_name,
            ),
        ),
        deps = inherited_link_deps,
    )

    providers.append(linkable_graph)

    providers.append(merge_link_group_lib_info(deps = inherited_link_deps))

    return providers

# Compute transitive deps. Caller decides whether this is necessary.
def _compute_transitive_deps(
        ctx: AnalysisContext,
        dep_link_style: LinkStyle) -> (
    dict[Artifact, CrateName],
    dict[Artifact, CrateName],
    list[ArtifactTSet],
    dict[RustProcMacroMarker, ()],
):
    transitive_deps = {}
    transitive_rmeta_deps = {}
    external_debug_info = []
    transitive_proc_macro_deps = {}

    for dep in resolve_rust_deps(ctx):
        if dep.proc_macro_marker != None:
            transitive_proc_macro_deps[dep.proc_macro_marker] = ()

            # We don't want to propagate proc macros directly, and they have no transitive deps
            continue
        style = style_info(dep.info, dep_link_style)
        transitive_deps[style.rlib] = dep.info.crate
        transitive_deps.update(style.transitive_deps)

        transitive_rmeta_deps[style.rmeta] = dep.info.crate
        transitive_rmeta_deps.update(style.transitive_rmeta_deps)

        external_debug_info.append(style.external_debug_info)

        transitive_proc_macro_deps.update(style.transitive_proc_macro_deps)

    return transitive_deps, transitive_rmeta_deps, external_debug_info, transitive_proc_macro_deps

def rust_library_macro_wrapper(rust_library: typing.Callable) -> typing.Callable:
    def wrapper(**kwargs):
        if not kwargs.pop("_use_legacy_proc_macros", False) and kwargs.get("proc_macro") == True:
            name = kwargs["name"]
            if kwargs.get("crate", None) == None and kwargs.get("crate_dynamic", None) == None:
                kwargs["crate"] = name.replace("-", "_")

            rust_proc_macro_alias(
                name = name,
                actual_exec = ":_" + name,
                actual_plugin = ":_" + name,
                visibility = kwargs.pop("visibility", []),
            )
            kwargs["name"] = "_" + name

        rust_library(**kwargs)

    return wrapper

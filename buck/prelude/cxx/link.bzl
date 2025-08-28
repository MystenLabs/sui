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
    "project_artifacts",
)
load(
    "@prelude//cxx:cxx_bolt.bzl",
    "bolt",
    "cxx_use_bolt",
)
load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxToolchainInfo")
load(
    "@prelude//cxx/dist_lto:dist_lto.bzl",
    "cxx_dist_link",
)
load("@prelude//linking:execution_preference.bzl", "LinkExecutionPreference", "LinkExecutionPreferenceInfo", "get_action_execution_attributes")
load(
    "@prelude//linking:link_info.bzl",
    "ArchiveLinkable",
    "LinkArgs",
    "LinkOrdering",
    "LinkedObject",
    "ObjectsLinkable",
    "unpack_external_debug_info",
    "unpack_link_args",
)
load(
    "@prelude//linking:lto.bzl",
    "get_split_debug_lto_info",
)
load("@prelude//linking:strip.bzl", "strip_object")
load("@prelude//utils:utils.bzl", "expect", "map_val", "value_or")
load(
    ":anon_link.bzl",
    "ANON_ATTRS",
    "deserialize_anon_attrs",
    "serialize_anon_attrs",
)
load(":bitcode.bzl", "make_bitcode_bundle")
load(":cxx_context.bzl", "get_cxx_toolchain_info")
load(
    ":cxx_link_utility.bzl",
    "cxx_link_cmd_parts",
    "generates_split_debug",
    "linker_map_args",
    "make_link_args",
)
load(":dwp.bzl", "dwp", "dwp_available")
load(":link_types.bzl", "CxxLinkResultType", "LinkOptions", "merge_link_options")
load(
    ":linker.bzl",
    "SharedLibraryFlagOverrides",  # @unused Used as a type
    "get_import_library",
    "get_output_flags",
    "get_shared_library_flags",
    "get_shared_library_name_linker_flags",
)

CxxLinkerMapData = record(
    map = field(Artifact),
    binary = field(Artifact),
)

CxxLinkResult = record(
    # The resulting artifact from the link
    linked_object = LinkedObject,
    linker_map_data = [CxxLinkerMapData, None],
    link_execution_preference_info = LinkExecutionPreferenceInfo,
)

def link_external_debug_info(
        ctx: AnalysisContext,
        links: list[LinkArgs],
        split_debug_output: [Artifact, None] = None,
        pdb: [Artifact, None] = None) -> ArtifactTSet:
    external_debug_artifacts = []

    # When using LTO+split-dwarf, the link step will generate externally
    # referenced debug info.
    if split_debug_output != None:
        external_debug_artifacts.append(split_debug_output)

    if pdb != None:
        external_debug_artifacts.append(pdb)

    external_debug_infos = []

    # Add-in an externally referenced debug info that the linked object may
    # reference (and which may need to be available for debugging).
    for link in links:
        external_debug_infos.append(unpack_external_debug_info(ctx.actions, link))

    return make_artifact_tset(
        actions = ctx.actions,
        label = ctx.label,
        artifacts = external_debug_artifacts,
        children = external_debug_infos,
    )

# Actually perform a link into the supplied output.
def cxx_link_into(
        ctx: AnalysisContext,
        # The destination for the link output.
        output: Artifact,
        result_type: CxxLinkResultType,
        opts: LinkOptions) -> CxxLinkResult:
    cxx_toolchain_info = opts.cxx_toolchain or get_cxx_toolchain_info(ctx)
    linker_info = cxx_toolchain_info.linker_info

    should_generate_dwp = dwp_available(cxx_toolchain_info)
    is_result_executable = result_type.value == "executable"

    if linker_info.generate_linker_maps:
        linker_map = ctx.actions.declare_output(output.short_path + "-LinkMap.txt")
        linker_map_data = CxxLinkerMapData(
            map = linker_map,
            binary = output,
        )
    else:
        linker_map = None
        linker_map_data = None

    if linker_info.supports_distributed_thinlto and opts.enable_distributed_thinlto:
        if not linker_info.requires_objects:
            fail("Cannot use distributed thinlto if the cxx toolchain doesn't require_objects")
        exe = cxx_dist_link(
            ctx,
            opts.links,
            output,
            linker_map,
            opts.category_suffix,
            opts.identifier,
            should_generate_dwp,
            is_result_executable,
        )
        return CxxLinkResult(
            linked_object = exe,
            linker_map_data = linker_map_data,
            link_execution_preference_info = LinkExecutionPreferenceInfo(
                preference = opts.link_execution_preference,
            ),
        )

    if linker_info.generate_linker_maps:
        links_with_linker_map = opts.links + [linker_map_args(cxx_toolchain_info, linker_map.as_output())]
    else:
        links_with_linker_map = opts.links

    linker, toolchain_linker_flags = cxx_link_cmd_parts(cxx_toolchain_info)
    all_link_args = cmd_args(toolchain_linker_flags)
    all_link_args.add(get_output_flags(linker_info.type, output))

    # Darwin LTO requires extra link outputs to preserve debug info
    split_debug_output = None
    split_debug_lto_info = get_split_debug_lto_info(ctx.actions, cxx_toolchain_info, output.short_path)
    if split_debug_lto_info != None:
        all_link_args.add(split_debug_lto_info.linker_flags)
        split_debug_output = split_debug_lto_info.output
    expect(not generates_split_debug(cxx_toolchain_info) or split_debug_output != None)

    link_args_suffix = None
    if opts.identifier:
        link_args_suffix = opts.identifier
    if opts.category_suffix:
        if link_args_suffix:
            link_args_suffix += "-" + opts.category_suffix
        else:
            link_args_suffix = opts.category_suffix
    link_args_output = make_link_args(
        ctx.actions,
        cxx_toolchain_info,
        links_with_linker_map,
        suffix = link_args_suffix,
        output_short_path = output.short_path,
        is_shared = result_type.value == "shared_library",
        link_ordering = value_or(
            opts.link_ordering,
            # Fallback to toolchain default.
            map_val(LinkOrdering, linker_info.link_ordering),
        ),
    )
    all_link_args.add(link_args_output.link_args)

    bitcode_linkables = []
    for link_item in opts.links:
        if link_item.infos == None:
            continue
        for link_info in link_item.infos:
            for linkable in link_info.linkables:
                if isinstance(linkable, ArchiveLinkable) or isinstance(linkable, ObjectsLinkable):
                    if linkable.bitcode_bundle != None:
                        bitcode_linkables.append(linkable.bitcode_bundle)

    if len(bitcode_linkables) > 0:
        bitcode_artifact = make_bitcode_bundle(ctx, output.short_path + ".bc", bitcode_linkables)
    else:
        bitcode_artifact = None

    external_debug_info = link_external_debug_info(
        ctx = ctx,
        links = opts.links,
        split_debug_output = split_debug_output,
        pdb = link_args_output.pdb_artifact,
    )

    if linker_info.type == "windows":
        shell_quoted_args = cmd_args(all_link_args)
    else:
        shell_quoted_args = cmd_args(all_link_args, quote = "shell")

    argfile, _ = ctx.actions.write(
        output.short_path + ".linker.argsfile",
        shell_quoted_args,
        allow_args = True,
    )

    command = cmd_args(linker)
    command.add(cmd_args(argfile, format = "@{}"))
    command.hidden(link_args_output.hidden)
    command.hidden(shell_quoted_args)
    category = "cxx_link"
    if opts.category_suffix != None:
        category += "_" + opts.category_suffix

    # If the linked object files don't contain debug info, clang may not
    # generate a DWO directory, so make sure we at least `mkdir` and empty
    # one to make v2/RE happy.
    if split_debug_output != None:
        cmd = cmd_args(["/bin/sh", "-c"])
        cmd.add(cmd_args(split_debug_output.as_output(), format = 'mkdir -p {}; "$@"'))
        cmd.add('""').add(command)
        cmd.hidden(command)
        command = cmd

    link_execution_preference_info = LinkExecutionPreferenceInfo(
        preference = opts.link_execution_preference,
    )
    action_execution_properties = get_action_execution_attributes(
        opts.link_execution_preference,
    )

    ctx.actions.run(
        command,
        prefer_local = action_execution_properties.prefer_local,
        prefer_remote = action_execution_properties.prefer_remote,
        local_only = action_execution_properties.local_only,
        weight = opts.link_weight,
        category = category,
        identifier = opts.identifier,
        force_full_hybrid_if_capable = action_execution_properties.full_hybrid,
        allow_cache_upload = opts.allow_cache_upload,
    )
    unstripped_output = output
    if opts.strip:
        strip_args = opts.strip_args_factory(ctx) if opts.strip_args_factory else cmd_args()
        output = strip_object(ctx, cxx_toolchain_info, output, strip_args, opts.category_suffix)

    final_output = output if not (is_result_executable and cxx_use_bolt(ctx)) else bolt(ctx, output, opts.identifier)
    dwp_artifact = None
    if should_generate_dwp:
        # TODO(T110378144): Once we track split dwarf from compiles, we should
        # just pass in `binary.external_debug_info` here instead of all link
        # args.
        dwp_inputs = cmd_args()
        for link in opts.links:
            dwp_inputs.add(unpack_link_args(link))
        dwp_inputs.add(project_artifacts(ctx.actions, [external_debug_info]))

        dwp_artifact = dwp(
            ctx,
            cxx_toolchain_info,
            final_output,
            identifier = opts.identifier,
            category_suffix = opts.category_suffix,
            # TODO(T110378142): Ideally, referenced objects are a list of
            # artifacts, but currently we don't track them properly.  So, we
            # just pass in the full link line and extract all inputs from that,
            # which is a bit of an overspecification.
            referenced_objects = [dwp_inputs],
        )

    linked_object = LinkedObject(
        output = final_output,
        bitcode_bundle = bitcode_artifact.artifact if bitcode_artifact else None,
        prebolt_output = output,
        unstripped_output = unstripped_output,
        dwp = dwp_artifact,
        external_debug_info = external_debug_info,
        linker_argsfile = argfile,
        linker_filelist = link_args_output.filelist,
        linker_command = command,
        import_library = opts.import_library,
        pdb = link_args_output.pdb_artifact,
        split_debug_output = split_debug_output,
    )

    return CxxLinkResult(
        linked_object = linked_object,
        linker_map_data = linker_map_data,
        link_execution_preference_info = link_execution_preference_info,
    )

_AnonLinkInfo = provider(fields = {
    "result": provider_field(typing.Any, default = None),  # CxxLinkResult
})

# dwp and split_debug_output are optional outputs, but promise artifacts require an actual artifact
# when being resolved. Let's add some placeholders here so that we always generate an artifact when
# applying the map functions.
_AnonLinkInfoPlaceholder = provider(fields = {
    "dwp": provider_field(typing.Any),
    "split_debug_output": provider_field(typing.Any),
})

def _anon_link_impl(ctx):
    (output, result_type, opts) = deserialize_anon_attrs(ctx.actions, ctx.label, ctx.attrs)

    link_result = cxx_link(
        ctx = ctx,
        output = output,
        result_type = result_type,
        opts = opts,
    )

    dwp_placeholder = ctx.actions.write("placeholder_dwp", "")
    split_debug_output_placeholder = ctx.actions.write("placeholder_split_debug_output", "")

    return [
        DefaultInfo(),
        _AnonLinkInfo(result = link_result),
        _AnonLinkInfoPlaceholder(dwp = dwp_placeholder, split_debug_output = split_debug_output_placeholder),
    ]

_anon_link_rule = anon_rule(
    impl = _anon_link_impl,
    attrs = ANON_ATTRS,
    artifact_promise_mappings = {
        "dwp": lambda p: _get_link_artifact(p, "dwp"),
        "output": lambda p: p[_AnonLinkInfo].result.linked_object.output,
        "split_debug_output": lambda p: _get_link_artifact(p, "split_debug_output"),
    },
)

def _get_link_artifact(p: ProviderCollection, name: str) -> Artifact:
    linked_object = p[_AnonLinkInfo].result.linked_object
    if getattr(linked_object, name) != None:
        return getattr(linked_object, name)
    else:
        return getattr(p[_AnonLinkInfoPlaceholder], name)

def _anon_cxx_link(
        ctx: AnalysisContext,
        output: str,
        result_type: CxxLinkResultType,
        opts: LinkOptions) -> CxxLinkResult:
    if opts.cxx_toolchain:
        fail("anon link requires getting toolchain from ctx.attrs._cxx_toolchain")
    cxx_toolchain = ctx.attrs._cxx_toolchain[CxxToolchainInfo]
    anon_link_target = ctx.actions.anon_target(
        _anon_link_rule,
        dict(
            _cxx_toolchain = ctx.attrs._cxx_toolchain,
            **serialize_anon_attrs(
                output = output,
                result_type = result_type,
                opts = opts,
            )
        ),
    )

    dwp = None
    if dwp_available(cxx_toolchain):
        dwp = anon_link_target.artifact("dwp")

    split_debug_output = None
    if generates_split_debug(cxx_toolchain):
        split_debug_output = anon_link_target.artifact("split_debug_output")

    output = ctx.actions.assert_short_path(anon_link_target.artifact("output"), short_path = output)

    external_debug_info = link_external_debug_info(
        ctx = ctx,
        links = opts.links,
        split_debug_output = split_debug_output,
    )

    return CxxLinkResult(
        linked_object = LinkedObject(
            output = output,
            unstripped_output = output,
            dwp = dwp,
            external_debug_info = external_debug_info,
        ),
        linker_map_data = None,
        link_execution_preference_info = LinkExecutionPreferenceInfo(
            preference = LinkExecutionPreference("any"),
        ),
    )

def cxx_link(
        ctx: AnalysisContext,
        output: str,
        result_type: CxxLinkResultType,
        opts: LinkOptions,
        anonymous: bool = False):
    if anonymous:
        return _anon_cxx_link(
            ctx = ctx,
            output = output,
            result_type = result_type,
            opts = opts,
        )
    return cxx_link_into(
        ctx = ctx,
        output = ctx.actions.declare_output(output),
        result_type = result_type,
        opts = opts,
    )

def cxx_link_shared_library(
        ctx: AnalysisContext,
        # The destination for the link output.
        output: str,
        opts: LinkOptions,
        # Optional soname to link into shared library.
        name: [str, None] = None,
        # Overrides the default flags used to specify building shared libraries
        shared_library_flags: [SharedLibraryFlagOverrides, None] = None,
        anonymous: bool = False) -> CxxLinkResult:
    # links: list[LinkArgs] = [],
    # link_execution_preference: LinkExecutionPreference = LinkExecutionPreference("any"),

    """
    Link a shared library into the supplied output.
    """
    cxx_toolchain = opts.cxx_toolchain or get_cxx_toolchain_info(ctx)
    linker_info = cxx_toolchain.linker_info
    linker_type = linker_info.type
    extra_args = []

    extra_args.extend(get_shared_library_flags(linker_type, shared_library_flags))  # e.g. "-shared"
    if name != None:
        extra_args.extend(get_shared_library_name_linker_flags(linker_type, name, shared_library_flags))

    link_execution_preference = opts.link_execution_preference
    if linker_info.link_libraries_locally:
        link_execution_preference = LinkExecutionPreference("local")

    (import_library, import_library_args) = get_import_library(
        ctx,
        linker_type,
        output,
    )

    links_with_extra_args = [LinkArgs(flags = extra_args)] + opts.links + [LinkArgs(flags = import_library_args)]

    opts = merge_link_options(
        opts,
        links = links_with_extra_args,
        link_execution_preference = link_execution_preference,
        import_library = import_library,
    )

    return cxx_link(
        ctx = ctx,
        output = output,
        result_type = CxxLinkResultType("shared_library"),
        opts = opts,
        anonymous = anonymous,
    )

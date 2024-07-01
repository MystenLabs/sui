# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//:artifact_tset.bzl",
    "ArtifactInfo",
    "make_artifact_tset",
)
load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxToolchainInfo")
load("@prelude//linking:execution_preference.bzl", "LinkExecutionPreference")
load(
    "@prelude//linking:link_info.bzl",
    "Archive",
    "ArchiveLinkable",
    "LinkArgs",
    "LinkInfo",  # @unused Used as a type
    "ObjectsLinkable",
    "SharedLibLinkable",
)
load(
    ":link_types.bzl",
    "CxxLinkResultType",
    "LinkOptions",
    "link_options",
)

def _serialize_linkable(linkable):
    if isinstance(linkable, ArchiveLinkable):
        return ("archive", (
            (linkable.archive.artifact, linkable.archive.external_objects),
            linkable.link_whole,
            linkable.linker_type,
            linkable.supports_lto,
        ))

    if isinstance(linkable, ObjectsLinkable):
        return ("objects", (
            linkable.objects,
            linkable.link_whole,
            linkable.linker_type,
        ))

    if isinstance(linkable, SharedLibLinkable):
        return ("shared", (
            linkable.lib,
            linkable.link_without_soname,
        ))

    fail("cannot serialize linkable \"{}\"".format(str(linkable)))

def _serialize_link_info(info: LinkInfo):
    external_debug_info = []
    if info.external_debug_info._tset != None:
        for infos in info.external_debug_info._tset.traverse():
            external_debug_info.extend(infos)
        external_debug_info = dedupe(external_debug_info)
    return (
        info.name,
        info.pre_flags,
        info.post_flags,
        [_serialize_linkable(linkable) for linkable in info.linkables],
        # TODO(agallagher): It appears anon-targets don't allow passing in `label`.
        [(str(info.label.raw_target()), info.artifacts) for info in external_debug_info],
    )

def _serialize_link_args(link: LinkArgs):
    if link.flags != None:
        return ("flags", link.flags)

    if link.infos != None:
        return ("infos", [_serialize_link_info(info) for info in link.infos])

    fail("cannot serialize link args")

def _serialize_links(links: list[LinkArgs]):
    return [_serialize_link_args(link) for link in links]

def serialize_anon_attrs(
        output: str,
        result_type: CxxLinkResultType,
        opts: LinkOptions) -> dict[str, typing.Any]:
    return dict(
        links = _serialize_links(opts.links),
        output = output,
        import_library = opts.import_library,
        link_execution_preference = opts.link_execution_preference.value,
        enable_distributed_thinlto = opts.enable_distributed_thinlto,
        identifier = opts.identifier,
        category_suffix = opts.category_suffix,
        result_type = result_type.value,
        allow_cache_upload = opts.allow_cache_upload,
    )

def _deserialize_linkable(linkable: (str, typing.Any)) -> typing.Any:
    typ, payload = linkable

    if typ == "archive":
        (artifact, external_objects), link_whole, linker_type, supports_lto = payload
        return ArchiveLinkable(
            archive = Archive(
                artifact = artifact,
                external_objects = external_objects,
            ),
            link_whole = link_whole,
            linker_type = linker_type,
            supports_lto = supports_lto,
        )

    if typ == "objects":
        objects, link_whole, linker_type = payload
        return ObjectsLinkable(
            objects = objects,
            link_whole = link_whole,
            linker_type = linker_type,
        )

    if typ == "shared":
        lib, link_without_soname = payload
        return SharedLibLinkable(
            lib = lib,
            link_without_soname = link_without_soname,
        )

    fail("Invalid linkable type: {}".format(typ))

def _deserialize_link_info(actions: AnalysisActions, label: Label, info) -> LinkInfo:
    name, pre_flags, post_flags, linkables, external_debug_info = info
    return LinkInfo(
        name = name,
        pre_flags = pre_flags,
        post_flags = post_flags,
        linkables = [_deserialize_linkable(linkable) for linkable in linkables],
        external_debug_info = make_artifact_tset(
            actions = actions,
            infos = [
                ArtifactInfo(label = label, artifacts = artifacts)
                for _label, artifacts in external_debug_info
            ],
        ),
    )

def _deserialize_link_args(
        actions: AnalysisActions,
        label: Label,
        link: (str, typing.Any)) -> LinkArgs:
    typ, payload = link

    if typ == "flags":
        return LinkArgs(flags = payload)

    if typ == "infos":
        return LinkArgs(infos = [_deserialize_link_info(actions, label, info) for info in payload])

    fail("invalid link args type: {}".format(typ))

def deserialize_anon_attrs(
        actions: AnalysisActions,
        label: Label,
        attrs: struct) -> (str, CxxLinkResultType, LinkOptions):
    opts = link_options(
        links = [_deserialize_link_args(actions, label, link) for link in attrs.links],
        import_library = attrs.import_library,
        link_execution_preference = LinkExecutionPreference(attrs.link_execution_preference),
        category_suffix = attrs.category_suffix,
        identifier = attrs.identifier,
        enable_distributed_thinlto = attrs.enable_distributed_thinlto,
        allow_cache_upload = attrs.allow_cache_upload,
    )

    result_type = CxxLinkResultType(attrs.result_type)

    return (attrs.output, result_type, opts)

# The attributes -- and their serialzied type -- that can be passed to an
# anonymous link.
ANON_ATTRS = {
    "allow_cache_upload": attrs.bool(),
    "category_suffix": attrs.string(),
    "enable_distributed_thinlto": attrs.bool(),
    "identifier": attrs.option(attrs.string(), default = None),
    "import_library": attrs.option(attrs.source(), default = None),
    "link_execution_preference": attrs.enum(LinkExecutionPreference.values()),
    "links": attrs.list(
        # List[LinkArgs]
        attrs.tuple(
            attrs.enum(["flags", "infos"]),  # "flags" or "infos"
            attrs.one_of(
                attrs.list(attrs.one_of(attrs.string(), attrs.arg())),  # flags
                attrs.list(
                    # infos: [LinkInfo]
                    attrs.tuple(
                        # LinkInfo
                        attrs.option(attrs.string(), default = None),  # name
                        attrs.list(attrs.one_of(attrs.string(), attrs.arg())),  # pre_flags
                        attrs.list(attrs.one_of(attrs.string(), attrs.arg())),  # post_flags
                        attrs.list(
                            # linkables
                            attrs.tuple(
                                attrs.enum(["archive", "objects", "shared"]),
                                attrs.one_of(
                                    attrs.tuple(
                                        # ObjectsLinkable
                                        attrs.list(attrs.source()),  # objects
                                        attrs.bool(),  # link_whole
                                        attrs.string(),  # linker_type
                                    ),
                                    attrs.tuple(
                                        # ArchiveLinkable
                                        attrs.tuple(
                                            # Archive
                                            attrs.source(),  # archive
                                            attrs.list(attrs.source()),  # external_objects
                                        ),
                                        attrs.bool(),  # link_whole
                                        attrs.string(),  # linker_type
                                        attrs.bool(),  # supports_lto
                                    ),
                                    attrs.tuple(
                                        # SharedLibLinkable
                                        attrs.source(),  # lib
                                        attrs.bool(),  # link_without_soname
                                    ),
                                ),
                            ),
                        ),
                        attrs.list(
                            # external_debug_info
                            attrs.tuple(
                                # TODO(agallagher): It appears anon-targets don't
                                # allow passing in `label`.
                                attrs.string(),  # label
                                attrs.list(attrs.source()),  # artifacts
                            ),
                        ),
                    ),
                ),
            ),
        ),
        default = [],
    ),
    "output": attrs.string(),
    "result_type": attrs.enum(CxxLinkResultType.values()),
    "_cxx_toolchain": attrs.dep(providers = [CxxToolchainInfo]),
}

# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Providers for OCaml build rules.

load("@prelude//utils:utils.bzl", "flatten")

OCamlToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "ocaml_compiler": provider_field(typing.Any, default = None),
        # [Note: What is `binutils_ld`?]
        # ------------------------------
        # When the compiler is invoked in partial linking mode (`ocamlopt.opt
        # -output-obj ...`) it makes naked calls to `ld`. In such a case, if we
        # don't arrange to execute `ocamlopt.opt` in an environment where `ld`
        # resolves to an `ld` that is either the `ld` in scope when the compiler was
        # built (or at least compatible with it) it then a call to the system `ld`
        # will result and almost certainly be a mismatch.
        #
        # How the compiler invokes `ld` is established when it's built by its
        # configure script
        # (https://github.com/ocaml/ocaml/blob/f27d671b23f5246d37d91571eeccb802d5399a0b/configure.ac).
        #
        # So far I've found `fbcode//.../binutils:bin/ld` to be a choice that works.
        # It's in `_mk_ocaml_opt` in `ocaml.bzl` where we make use of this.
        "binutils_ld": provider_field(typing.Any, default = None),
        "binutils_as": provider_field(typing.Any, default = None),
        "dep_tool": provider_field(typing.Any, default = None),
        "yacc_compiler": provider_field(typing.Any, default = None),
        "menhir_compiler": provider_field(typing.Any, default = None),
        "lex_compiler": provider_field(typing.Any, default = None),
        "libc": provider_field(typing.Any, default = None),  # MergedLinkInfo of libc
        "ocaml_bytecode_compiler": provider_field(typing.Any, default = None),
        "debug": provider_field(typing.Any, default = None),
        "interop_includes": provider_field(typing.Any, default = None),
        "warnings_flags": provider_field(typing.Any, default = None),
        "ocaml_compiler_flags": provider_field(typing.Any, default = None),  # passed to both ocamlc and ocamlopt, like dune's (flags)
        "ocamlc_flags": provider_field(typing.Any, default = None),  # passed to ocamlc only, like dune's (ocamlc_flags)
        "ocamlopt_flags": provider_field(typing.Any, default = None),  # passed to ocamlopt only, like dune's (ocamlopt_flags)
        "runtime_dep_link_flags": provider_field(typing.Any, default = None),  # `ocaml_object` uses this to satisfy OCaml runtimes C link dependencies
        "runtime_dep_link_extras": provider_field(typing.Any, default = None),  # `ocaml_object` uses this to satisfy OCaml runtimes C link dependencies
    },
)

# Stores "platform"/flavor name used to resolve *platform_* arguments
OCamlPlatformInfo = provider(fields = {
    "name": provider_field(typing.Any, default = None),
})

# A list of `OCamlLibraryInfo`s.
OCamlLinkInfo = provider(
    # Contains a list of OCamlLibraryInfo records
    fields = {"info": provider_field(typing.Any, default = None)},
)

# A record of an OCaml library.
OCamlLibraryInfo = record(
    # The library target name: e.g. "`foo`"
    name = str,
    # The full library target: e.g. "`fbcode//...:foo`"
    target = Label,
    # .a (C archives e.g. `libfoo_stubs.a`)
    c_libs = list[Artifact],
    # .o (Native compiler produced stubs)
    stbs_nat = list[Artifact],
    # .o (Bytecode compiler produced stubs)
    stbs_byt = list[Artifact],
    # .cma (Bytecode compiler module archives e.g. `libfoo.cma`)
    cmas = list[Artifact],
    # .cmxa (Native compiler module archives e.g. `libfoo.cmxa`)
    cmxas = list[Artifact],
    # .cmi (Native compiled module interfaces)
    cmis_nat = list[Artifact],
    # .cmi (Bytecode compiled module interfaces)
    cmis_byt = list[Artifact],
    # .cmo (Bytecode compiled modules - bytecode)
    cmos = list[Artifact],
    # .cmx (Compiled modules - native)
    cmxs = list[Artifact],
    # .cmt (Native compiler produced typed abstract syntax trees)
    cmts_nat = list[Artifact],
    # .cmt (Bytecode compiler produced typed abstract syntax trees)
    cmts_byt = list[Artifact],
    # .cmti (Native compiler produced typed abstract syntax trees)
    cmtis_nat = list[Artifact],
    # .cmti (Bytecode compiler produced typed abstract syntax trees)
    cmtis_byt = list[Artifact],
    # Compile flags for native clients who use this library.
    include_dirs_nat = list[cmd_args],
    # Compile flags for bytecode clients who use this library.
    include_dirs_byt = list[cmd_args],
    # Native C libs (like `libthreadsnat.a` in the compiler's `threads` package)
    native_c_libs = list[Artifact],
    # Bytecode C libs (like `libthreads.a` in the compiler's `threads` package)
    bytecode_c_libs = list[Artifact],
)

def merge_ocaml_link_infos(lis: list[OCamlLinkInfo]) -> OCamlLinkInfo:
    return OCamlLinkInfo(info = dedupe(flatten([li.info for li in lis])))

def project_expand(value: dict[str, list[Artifact]]):
    return value["expand"]

def project_ide(value: dict[str, list[Artifact]]):
    return value["ide"]

def project_bytecode(value: dict[str, list[Artifact]]):
    return value["bytecode"]

OtherOutputsTSet = transitive_set(
    args_projections = {"bytecode": project_bytecode, "expand": project_expand, "ide": project_ide},
)

OtherOutputsInfo = provider(
    fields = {"info": provider_field(typing.Any, default = None)},  # :OtherOutputsTSet
)

def merge_other_outputs_info(ctx: AnalysisContext, value: dict[str, list[Artifact]], infos: list[OtherOutputsInfo]) -> OtherOutputsInfo:
    return OtherOutputsInfo(
        info =
            ctx.actions.tset(
                OtherOutputsTSet,
                value = value,
                children = [p.info for p in infos],
            ),
    )

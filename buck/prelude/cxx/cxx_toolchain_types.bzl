# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:debug.bzl", "SplitDebugMode")

# For cases where our `ld` dependency provides more than an executable and
# would like to give us flags too. We use this to place the flags in the proper
# field (linker_flags), so that things that want ldflags without the linker
# executable can access those.
RichLinkerRunInfo = provider(fields = {"exe": provider_field(typing.Any, default = None), "flags": provider_field(typing.Any, default = None)})

LinkerType = ["gnu", "darwin", "windows", "wasm"]

ShlibInterfacesMode = enum("disabled", "enabled", "defined_only")

# TODO(T110378149): Consider whether it makes sense to move these things to
# configurations/constraints rather than part of the toolchain.
LinkerInfo = provider(
    # @unsorted-dict-items
    fields = {
        "archiver": provider_field(typing.Any, default = None),
        "archiver_flags": provider_field(typing.Any, default = None),
        "archiver_supports_argfiles": provider_field(typing.Any, default = None),
        "archiver_type": provider_field(typing.Any, default = None),
        "archive_contents": provider_field(typing.Any, default = None),
        "archive_objects_locally": provider_field(typing.Any, default = None),
        # "archiver_platform",
        # "" on Unix, "exe" on Windows
        "binary_extension": provider_field(typing.Any, default = None),  # str
        "generate_linker_maps": provider_field(typing.Any, default = None),  # bool
        # Whether to run native links locally.  We support this for fbcode platforms
        # to avoid issues with C++ static links (see comment in
        # `platform/cxx_toolchains.bzl` for details).
        "link_binaries_locally": provider_field(typing.Any, default = None),
        # Whether to run native shared library links locally. For certain use cases
        # (e.g., large Apple frameworks), it's more efficient to link locally due
        # GiBs of object files (which can also lead to RE errors/timesouts etc).
        "link_libraries_locally": provider_field(typing.Any, default = None),
        "link_style": provider_field(typing.Any, default = None),  # LinkStyle
        "link_weight": provider_field(typing.Any, default = None),  # int
        "link_ordering": provider_field(typing.Any, default = None),  # LinkOrdering
        "linker": provider_field(typing.Any, default = None),
        "linker_flags": provider_field(typing.Any, default = None),
        "lto_mode": provider_field(typing.Any, default = None),
        "mk_shlib_intf": provider_field(typing.Any, default = None),
        # "o" on Unix, "obj" on Windows
        "object_file_extension": provider_field(typing.Any, default = None),  # str
        "shlib_interfaces": provider_field(ShlibInterfacesMode),
        "shared_dep_runtime_ld_flags": provider_field(typing.Any, default = None),
        # "lib" on Linux/Mac/Android, "" on Windows.
        "shared_library_name_default_prefix": provider_field(typing.Any, default = None),  # str
        # "{}.so" on Linux, "{}.dylib" on Mac, "{}.dll" on Windows
        "shared_library_name_format": provider_field(typing.Any, default = None),  # str
        "shared_library_versioned_name_format": provider_field(typing.Any, default = None),  # str
        "static_dep_runtime_ld_flags": provider_field(typing.Any, default = None),
        # "a" on Unix, "lib" on Windows
        "static_library_extension": provider_field(typing.Any, default = None),  # str
        "static_pic_dep_runtime_ld_flags": provider_field(typing.Any, default = None),
        "requires_archives": provider_field(typing.Any, default = None),
        "requires_objects": provider_field(typing.Any, default = None),
        "supports_distributed_thinlto": provider_field(typing.Any, default = None),
        "independent_shlib_interface_linker_flags": provider_field(typing.Any, default = None),
        "type": provider_field(typing.Any, default = None),  # of "LinkerType" type
        "use_archiver_flags": provider_field(typing.Any, default = None),
        "force_full_hybrid_if_capable": provider_field(typing.Any, default = None),
        "is_pdb_generated": provider_field(typing.Any, default = None),  # bool
        "produce_interface_from_stub_shared_library": provider_field(typing.Any, default = None),  # bool
    },
)

BinaryUtilitiesInfo = provider(fields = {
    "bolt_msdk": provider_field(typing.Any, default = None),
    "dwp": provider_field(typing.Any, default = None),
    "nm": provider_field(typing.Any, default = None),
    "objcopy": provider_field(typing.Any, default = None),
    "ranlib": provider_field(typing.Any, default = None),
    "strip": provider_field(typing.Any, default = None),
})

StripFlagsInfo = provider(
    # @unsorted-dict-items
    fields = {
        "strip_debug_flags": provider_field(typing.Any, default = None),  # [["str"], None]
        "strip_non_global_flags": provider_field(typing.Any, default = None),  # [["str"], None]
        "strip_all_flags": provider_field(typing.Any, default = None),  # [["str"], None]
    },
)

DepTrackingMode = enum(
    # MAKEFILE corresponds to `gcc -MD -MF depfile ...` on *nix
    "makefile",
    # SHOW_INCLUDES corresponds to `cl.exe /showIncludes ...` on windows
    "show_includes",
    # SHOW_HEADERS corresponds to `clang/gcc -H ...` on *nix
    "show_headers",
    # Some compilers - like ml64 - do not produce information about included files
    "none",
)

# TODO(T110378147): There's a bunch of info encoded in random places in buck
# derived from information in these toolchains but hardcoded (for example,
# which file extensions are preprocessable/compilable). We should figure out
# how to move most of that into these toolchain infos.
# TODO(T110378146): The inclusion of compiler and preprocessor in here is really
# just a legacy thing that was never cleaned up. Historically, buck supported a
# mode where compilation was done in two, explicitly separate phases
# (preprocess and then compile). We don't support that today, and including
# both of these mostly just ends up with added complexity and with us
# duplicating flags in command lines.

# In cxx_library, we support a bunch of different types of files (ex. cuda),
# the toolchain for these follow this common pattern.
_compiler_fields = [
    "compiler",
    "compiler_type",
    "compiler_flags",
    "preprocessor",
    "preprocessor_type",
    "preprocessor_flags",
    "dep_files_processor",
]

HipCompilerInfo = provider(fields = _compiler_fields)
CudaCompilerInfo = provider(fields = _compiler_fields)
CCompilerInfo = provider(fields = _compiler_fields)
CxxCompilerInfo = provider(fields = _compiler_fields)
AsmCompilerInfo = provider(fields = _compiler_fields)
AsCompilerInfo = provider(fields = _compiler_fields)

DistLtoToolsInfo = provider(
    # @unsorted-dict-items
    fields = {"planner": provider_field(typing.Any, default = None), "opt": provider_field(typing.Any, default = None), "prepare": provider_field(typing.Any, default = None), "copy": provider_field(typing.Any, default = None)},
)

CxxObjectFormat = enum(
    "native",
    "bitcode",
    "embedded-bitcode",
    "swift",
)

# - For targets being built for x86_64, arm64, the fPIC flag isn't respected. Everything is fPIC.
# - For targets being built for Windows, nothing is fPIC. The flag is ignored.
# - There are many platforms (linux, etc.) where the fPIC flag is supported.
#
# As a result, we can end-up in a place where you pic + non-pic artifacts are requested
# but the platform will produce the exact same output (despite the different files).
#
# The PicBehavior is applied to avoid using link or output styles that are unsupported by a toolchain.
PicBehavior = enum(
    # Regardless of whether -fPIC is specified explicitly
    # every compiled artifact will have a position-independent representation.
    # This should be the the default when targeting x86_64 + arm64.
    "always_enabled",
    # The -fPIC flag is known and changes the compiled artifact.
    "supported",
    # The -fPIC flag is unknown to this platform.
    "not_supported",
)

# TODO(T110378094): We should consider if we can change this from a hardcoded
# list of compiler_info to something more general. We could maybe do a list of
# compiler_info where each one also declares what extensions it supports.
# TODO(T110378145): Could we split up this Info so that each of the compilers
# could be provided by different dependencies? That would allow a target to
# only depend on the compilers it actually needs.
CxxToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "conflicting_header_basename_allowlist": provider_field(typing.Any, default = None),
        "use_distributed_thinlto": provider_field(typing.Any, default = None),
        "header_mode": provider_field(typing.Any, default = None),
        "headers_as_raw_headers_mode": provider_field(typing.Any, default = None),
        "linker_info": provider_field(typing.Any, default = None),
        "object_format": provider_field(typing.Any, default = None),
        "binary_utilities_info": provider_field(typing.Any, default = None),
        "c_compiler_info": provider_field(typing.Any, default = None),
        "cxx_compiler_info": provider_field(typing.Any, default = None),
        "asm_compiler_info": provider_field(typing.Any, default = None),
        "as_compiler_info": provider_field(typing.Any, default = None),
        "hip_compiler_info": provider_field(typing.Any, default = None),
        "cuda_compiler_info": provider_field(typing.Any, default = None),
        "mk_comp_db": provider_field(typing.Any, default = None),
        "mk_hmap": provider_field(typing.Any, default = None),
        "llvm_link": provider_field(typing.Any, default = None),
        "dist_lto_tools_info": provider_field(typing.Any, default = None),
        "use_dep_files": provider_field(typing.Any, default = None),
        "clang_remarks": provider_field(typing.Any, default = None),
        "clang_trace": provider_field(typing.Any, default = None),
        "cpp_dep_tracking_mode": provider_field(typing.Any, default = None),
        "cuda_dep_tracking_mode": provider_field(typing.Any, default = None),
        "strip_flags_info": provider_field(typing.Any, default = None),
        "split_debug_mode": provider_field(typing.Any, default = None),
        "bolt_enabled": provider_field(typing.Any, default = None),
        "pic_behavior": provider_field(typing.Any, default = None),
        "dumpbin_toolchain_path": provider_field(typing.Any, default = None),
    },
)

# Stores "platform"/flavor name used to resolve *platform_* arguments
CxxPlatformInfo = provider(
    # @unsorted-dict-items
    fields = {
        "name": provider_field(typing.Any, default = None),
        # List of aliases used to resolve platform_deps
        "deps_aliases": provider_field(typing.Any, default = None),
    },
)

def _validate_linker_info(info: LinkerInfo):
    if info.requires_archives and info.requires_objects:
        fail("only one of `requires_archives` and `requires_objects` can be enabled")

    if info.supports_distributed_thinlto and not info.requires_objects:
        fail("distributed thinlto requires enabling `requires_objects`")

def is_bitcode_format(format: CxxObjectFormat) -> bool:
    return format in [CxxObjectFormat("bitcode"), CxxObjectFormat("embedded-bitcode")]

def cxx_toolchain_infos(
        platform_name,
        c_compiler_info,
        cxx_compiler_info,
        linker_info,
        binary_utilities_info,
        header_mode,
        headers_as_raw_headers_mode = None,
        conflicting_header_basename_allowlist = [],
        asm_compiler_info = None,
        as_compiler_info = None,
        hip_compiler_info = None,
        cuda_compiler_info = None,
        object_format = CxxObjectFormat("native"),
        mk_comp_db = None,
        mk_hmap = None,
        use_distributed_thinlto = False,
        use_dep_files = False,
        clang_remarks = None,
        clang_trace = False,
        cpp_dep_tracking_mode = DepTrackingMode("none"),
        cuda_dep_tracking_mode = DepTrackingMode("none"),
        strip_flags_info = None,
        dist_lto_tools_info: [DistLtoToolsInfo, None] = None,
        split_debug_mode = SplitDebugMode("none"),
        bolt_enabled = False,
        llvm_link = None,
        platform_deps_aliases = [],
        pic_behavior = PicBehavior("supported"),
        dumpbin_toolchain_path = None):
    """
    Creates the collection of cxx-toolchain Infos for a cxx toolchain.

    c and c++ compiler infos are required, as is a linker info. The rest are
    optional, and it will be an error if any cxx_library or other rules have srcs
    of those other types.
    """

    # TODO(T110378099): verify types of the inner info objects.
    _validate_linker_info(linker_info)

    toolchain_info = CxxToolchainInfo(
        conflicting_header_basename_allowlist = conflicting_header_basename_allowlist,
        header_mode = header_mode,
        headers_as_raw_headers_mode = headers_as_raw_headers_mode,
        linker_info = linker_info,
        llvm_link = llvm_link,
        binary_utilities_info = binary_utilities_info,
        c_compiler_info = c_compiler_info,
        cxx_compiler_info = cxx_compiler_info,
        asm_compiler_info = asm_compiler_info,
        as_compiler_info = as_compiler_info,
        hip_compiler_info = hip_compiler_info,
        cuda_compiler_info = cuda_compiler_info,
        mk_comp_db = mk_comp_db,
        mk_hmap = mk_hmap,
        object_format = object_format,
        dist_lto_tools_info = dist_lto_tools_info,
        use_distributed_thinlto = use_distributed_thinlto,
        use_dep_files = use_dep_files,
        clang_remarks = clang_remarks,
        clang_trace = clang_trace,
        cpp_dep_tracking_mode = cpp_dep_tracking_mode,
        cuda_dep_tracking_mode = cuda_dep_tracking_mode,
        strip_flags_info = strip_flags_info,
        split_debug_mode = split_debug_mode,
        bolt_enabled = bolt_enabled,
        pic_behavior = pic_behavior,
        dumpbin_toolchain_path = dumpbin_toolchain_path,
    )

    # Provide placeholder mappings, used primarily by cxx_genrule.
    # We don't support these buck1 placeholders since we can't take an argument.
    # $(ldflags-pic-filter <pattern>)
    # $(ldflags-shared-filter <pattern>)
    # $(ldflags-static-filter <pattern>)
    unkeyed_variables = {
        "ar": linker_info.archiver,
        "cc": c_compiler_info.compiler,
        "cflags": _shell_quote(c_compiler_info.compiler_flags),
        "cppflags": _shell_quote(c_compiler_info.preprocessor_flags),
        "cxx": cxx_compiler_info.compiler,
        "cxxflags": _shell_quote(cxx_compiler_info.compiler_flags),
        "cxxppflags": _shell_quote(cxx_compiler_info.preprocessor_flags),
        "ld": linker_info.linker,
        # NOTE(agallagher): The arg-less variants of the ldflags macro are
        # identical, and are just separate to match v1's behavior (ideally,
        # we just have a single `ldflags` macro for this case).
        "ldflags-shared": _shell_quote(linker_info.linker_flags),
        "ldflags-static": _shell_quote(linker_info.linker_flags),
        "ldflags-static-pic": _shell_quote(linker_info.linker_flags),
        # TODO(T110378148): $(platform-name) is almost unusued. Should we remove it?
        "platform-name": platform_name,
    }

    if as_compiler_info != None:
        unkeyed_variables["as"] = as_compiler_info.compiler
        unkeyed_variables["asflags"] = _shell_quote(as_compiler_info.compiler_flags)
        unkeyed_variables["asppflags"] = _shell_quote(as_compiler_info.preprocessor_flags)

    if cuda_compiler_info != None:
        unkeyed_variables["cuda"] = cuda_compiler_info.compiler
        unkeyed_variables["cudaflags"] = _shell_quote(cuda_compiler_info.compiler_flags)
    placeholders_info = TemplatePlaceholderInfo(unkeyed_variables = unkeyed_variables)
    return [toolchain_info, placeholders_info, CxxPlatformInfo(name = platform_name, deps_aliases = platform_deps_aliases)]

def _shell_quote(xs):
    return cmd_args(xs, quote = "shell")

# export these things under a single "cxx" struct
cxx = struct(
    LinkerType = LinkerType,
    LinkerInfo = LinkerInfo,
    BinaryUtilitiesInfo = BinaryUtilitiesInfo,
    HipCompilerInfo = HipCompilerInfo,
    CudaCompilerInfo = CudaCompilerInfo,
    CCompilerInfo = CCompilerInfo,
    CxxCompilerInfo = CxxCompilerInfo,
    AsmCompilerInfo = AsmCompilerInfo,
    AsCompilerInfo = AsCompilerInfo,
    CxxToolchainInfo = CxxToolchainInfo,
    CxxPlatformInfo = CxxPlatformInfo,
    StripFlagsInfo = StripFlagsInfo,
    DistLtoToolsInfo = DistLtoToolsInfo,
    cxx_toolchain_infos = cxx_toolchain_infos,
)

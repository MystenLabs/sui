# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

#####################################################################
# Providers

SwiftObjectFormat = enum(
    "object",
    "bc",
    "ir",
    "irgen",
    "object-embed-bitcode",
)

SwiftToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "architecture": provider_field(typing.Any, default = None),
        "can_toolchain_emit_obj_c_header_textually": provider_field(typing.Any, default = None),  # bool
        "uncompiled_swift_sdk_modules_deps": provider_field(typing.Any, default = None),  # {str: dependency} Expose deps of uncompiled Swift SDK modules.
        "uncompiled_clang_sdk_modules_deps": provider_field(typing.Any, default = None),  # {str: dependency} Expose deps of uncompiled Clang SDK modules.
        "compiler_flags": provider_field(typing.Any, default = None),
        "compiler": provider_field(typing.Any, default = None),
        "prefix_serialized_debugging_options": provider_field(typing.Any, default = None),  # bool
        "object_format": provider_field(typing.Any, default = None),  # "SwiftObjectFormat"
        "resource_dir": provider_field(typing.Any, default = None),  # "artifact",
        "sdk_path": provider_field(typing.Any, default = None),
        "swift_stdlib_tool_flags": provider_field(typing.Any, default = None),
        "swift_stdlib_tool": provider_field(typing.Any, default = None),
        "runtime_run_paths": provider_field(typing.Any, default = None),  # [str]
        "supports_swift_cxx_interoperability_mode": provider_field(typing.Any, default = None),  # bool
        "supports_swift_importing_objc_forward_declarations": provider_field(typing.Any, default = None),  # bool
        "supports_cxx_interop_requirement_at_import": provider_field(typing.Any, default = None),  # bool
        "mk_swift_comp_db": provider_field(typing.Any, default = None),
    },
)

# A provider that represents a non-yet-compiled SDK (Swift or Clang) module,
# and doesn't contain any artifacts because Swift toolchain isn't resolved yet.
SdkUncompiledModuleInfo = provider(fields = {
    "deps": provider_field(typing.Any, default = None),  # [Dependency]
    "input_relative_path": provider_field(typing.Any, default = None),  # A relative prefixed path to a textual swiftinterface/modulemap file within an SDK.
    "is_framework": provider_field(typing.Any, default = None),  # This is mostly needed for the generated Swift module map file.
    "is_swiftmodule": provider_field(typing.Any, default = None),  # If True then represents a swiftinterface, otherwise Clang's modulemap.
    "module_name": provider_field(typing.Any, default = None),  # A real name of a module, without distinguishing suffixes.
    "partial_cmd": provider_field(typing.Any, default = None),  # Partial arguments, required to compile a particular SDK module.
    "target": provider_field(typing.Any, default = None),  # A string of the compiler target triple to use for clang module deps, eg arm64-apple-ios16.4
})

WrappedSdkCompiledModuleInfo = provider(fields = {
    "clang_debug_info": provider_field(typing.Any, default = None),  # A tset of PCM artifacts
    "clang_deps": provider_field(typing.Any, default = None),  # A SwiftCompiledModuleTset of SwiftCompiledModuleInfo of transitive clang deps
    "swift_debug_info": provider_field(typing.Any, default = None),  # A tset of swiftmodule artifacts
    "swift_deps": provider_field(typing.Any, default = None),  # A SwiftCompiledModuleTset of SwiftCompiledModuleInfo of transitive swift deps
})

SdkSwiftOverlayInfo = provider(fields = {
    "overlays": provider_field(typing.Any, default = None),  # {str: [str]} A mapping providing a list of overlay module names for each underlying module
})

SwiftCompiledModuleInfo = provider(fields = {
    "clang_importer_args": provider_field(typing.Any, default = None),  # cmd_args of include flags for the clang importer.
    "is_framework": provider_field(typing.Any, default = None),
    "is_swiftmodule": provider_field(typing.Any, default = None),  # If True then contains a compiled swiftmodule, otherwise Clang's pcm.
    "module_name": provider_field(typing.Any, default = None),  # A real name of a module, without distinguishing suffixes.
    "output_artifact": provider_field(typing.Any, default = None),  # Compiled artifact either swiftmodule or pcm.
})

def _add_swiftmodule_search_path(module_info: SwiftCompiledModuleInfo):
    # We need to import the containing folder, not the file itself.
    return ["-I", cmd_args(module_info.output_artifact).parent()] if module_info.is_swiftmodule else []

def _add_clang_import_flags(module_info: SwiftCompiledModuleInfo):
    if module_info.is_swiftmodule:
        return []
    else:
        return [module_info.clang_importer_args]

def _swift_module_map_struct(module_info: SwiftCompiledModuleInfo):
    return struct(
        isFramework = module_info.is_framework,
        moduleName = module_info.module_name,
        modulePath = module_info.output_artifact,
    )

SwiftCompiledModuleTset = transitive_set(
    args_projections = {
        "clang_deps": _add_clang_import_flags,
        "module_search_path": _add_swiftmodule_search_path,
    },
    json_projections = {
        "swift_module_map": _swift_module_map_struct,
    },
)

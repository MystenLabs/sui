# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Input to build Python libraries and binaries (which are libraries wrapped in
# an executable). The various functions here must returns the inputs annotated
# below.
PythonLibraryInterface = record(
    # Shared libraries used by this Python library.
    # dict[str, SharedLibraryInfo]
    shared_libraries = field(typing.Callable),

    # An iterator of PythonLibraryManifests objects. This is used to collect extensions.
    # iterator of PythonLibraryManifests
    iter_manifests = field(typing.Callable),

    # A PythonLibraryManifestsInterface. This is used to convert manifests to
    # arguments for pexing. Unlike iter_manifests this allows for more
    # efficient calls, such as using t-sets projections.
    # PythonLibraryManifestsInterface
    manifests = field(typing.Callable),

    # Returns whether this Python library includes hidden resources.
    # bool
    has_hidden_resources = field(typing.Callable),

    # Converts the hidden resources in this Python library to arguments.
    # _arglike of hidden resources
    hidden_resources = field(typing.Callable),
)

PythonLibraryManifestsInterface = record(
    # Returns the source manifests for this Python library.
    # [_arglike] of source manifests
    src_manifests = field(typing.Callable),

    # Returns the files referenced by source manifests for this Python library.
    # [_arglike] of source artifacts
    src_artifacts = field(typing.Callable),
    src_artifacts_with_paths = field(typing.Callable),

    # Returns the source manifests for this Python library.
    # [_arglike] of source manifests
    src_type_manifests = field(typing.Callable),

    # Returns the files referenced by source manifests for this Python library.
    # [_arglike] of source artifacts
    src_type_artifacts = field(typing.Callable),
    src_type_artifacts_with_path = field(typing.Callable),

    # Returns the bytecode manifests for this Python library, given a PycInvalidationMode.
    # PycInvalidationMode -> [_arglike] of bytecode manifests (compiled with that mode)
    bytecode_manifests = field(typing.Callable),

    # Returns the files referenced by bytecode manifests for this Python library.
    # PycInvalidationMode -> [_arglike] of bytecode artifacts
    bytecode_artifacts = field(typing.Callable),
    # PycInvalidationMode -> [[artifact, _path]]
    bytecode_artifacts_with_paths = field(typing.Callable),

    # Returns the resources manifests for this Python library.
    # [_arglike] of resource manifests
    resource_manifests = field(typing.Callable),

    # Returns the files referenced by resource manifests for this Python library.
    # [_arglike] of resource artifacts
    resource_artifacts = field(typing.Callable),
    resource_artifacts_with_paths = field(typing.Callable),
)

# Entry point for Python binaries. First component designates if the second
# component is to be interpreted as a module or a function name.
EntryPointKind = enum("module", "function")
EntryPoint = (EntryPointKind, str)

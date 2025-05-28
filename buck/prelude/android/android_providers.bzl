# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//java:java_providers.bzl", "JavaLibraryInfo")

Aapt2LinkInfo = record(
    # "APK" containing resources to be used by the Android binary
    primary_resources_apk = Artifact,
    # proguard config needed to retain used resources
    proguard_config_file = Artifact,
    # R.txt containing all the linked resources
    r_dot_txt = Artifact,
)

PrebuiltNativeLibraryDir = record(
    raw_target = TargetLabel,
    dir = Artifact,  # contains subdirectories for different ABIs.
    for_primary_apk = bool,
    is_asset = bool,
)

ExopackageDexInfo = record(
    metadata = Artifact,
    directory = Artifact,
)

ExopackageNativeInfo = record(
    metadata = Artifact,
    directory = Artifact,
)

ExopackageResourcesInfo = record(
    assets = [Artifact, None],
    assets_hash = [Artifact, None],
    res = Artifact,
    res_hash = Artifact,
)

RDotJavaInfo = record(
    identifier = str,
    library_info = JavaLibraryInfo,
    source_zipped = Artifact,
)

AndroidBinaryNativeLibsInfo = record(
    apk_under_test_prebuilt_native_library_dirs = list[PrebuiltNativeLibraryDir],
    # Indicates which shared lib producing targets are included in the binary. Used by instrumentation tests
    # to exclude those from the test apk.
    apk_under_test_shared_libraries = list[TargetLabel],
    exopackage_info = ["ExopackageNativeInfo", None],
    root_module_native_lib_assets = list[Artifact],
    non_root_module_native_lib_assets = list[Artifact],
    native_libs_for_primary_apk = list[Artifact],
    generated_java_code = list[JavaLibraryInfo],
)

AndroidBinaryResourcesInfo = record(
    # Optional information about resources that should be exopackaged
    exopackage_info = [ExopackageResourcesInfo, None],
    # manifest to be used by the APK
    manifest = Artifact,
    # per-module manifests (packaged as assets)
    module_manifests = list[Artifact],
    # zip containing any strings packaged as assets
    packaged_string_assets = [Artifact, None],
    # "APK" containing resources to be used by the Android binary
    primary_resources_apk = Artifact,
    # proguard config needed to retain used resources
    proguard_config_file = Artifact,
    # R.java jars containing all the linked resources
    r_dot_java_infos = list[RDotJavaInfo],
    # directory containing filtered string resources files
    string_source_map = [Artifact, None],
    # directory containing filtered string resources files for Voltron language packs
    voltron_string_source_map = [Artifact, None],
    # list of jars that could contain resources that should be packaged into the APK
    jar_files_that_may_contain_resources = list[Artifact],
    # The resource infos that are used in this APK
    unfiltered_resource_infos = list["AndroidResourceInfo"],
)

# Information about an `android_build_config`
BuildConfigField = record(
    type = str,
    name = str,
    value = str,
)

AndroidBuildConfigInfo = provider(
    # @unsorted-dict-items
    fields = {
        "package": str,
        "build_config_fields": list[BuildConfigField],
    },
)

# Information about an `android_manifest`
AndroidManifestInfo = provider(
    fields = {
        "manifest": provider_field(typing.Any, default = None),  # artifact
        "merge_report": provider_field(typing.Any, default = None),  # artifact
    },
)

AndroidApkInfo = provider(
    fields = {
        "apk": provider_field(typing.Any, default = None),
        "manifest": provider_field(typing.Any, default = None),
    },
)

AndroidAabInfo = provider(
    fields = {
        "aab": provider_field(typing.Any, default = None),
        "manifest": provider_field(typing.Any, default = None),
    },
)

AndroidApkUnderTestInfo = provider(
    # @unsorted-dict-items
    fields = {
        "java_packaging_deps": provider_field(typing.Any, default = None),  # set_type("JavaPackagingDep")
        "keystore": provider_field(typing.Any, default = None),  # "KeystoreInfo"
        "manifest_entries": provider_field(typing.Any, default = None),  # dict
        "prebuilt_native_library_dirs": provider_field(typing.Any, default = None),  # set_type("PrebuiltNativeLibraryDir")
        "platforms": provider_field(typing.Any, default = None),  # [str]
        "primary_platform": provider_field(typing.Any, default = None),  # str
        "resource_infos": provider_field(typing.Any, default = None),  # set_type("ResourceInfos")
        "r_dot_java_packages": provider_field(typing.Any, default = None),  # set_type(str)
        "shared_libraries": provider_field(typing.Any, default = None),  # set_type(raw_target)
    },
)

AndroidInstrumentationApkInfo = provider(
    fields = {
        "apk_under_test": provider_field(typing.Any, default = None),  # "artifact"
    },
)

ManifestInfo = record(
    target_label = TargetLabel,
    manifest = Artifact,
)

def _artifacts(value: ManifestInfo):
    return value.manifest

AndroidBuildConfigInfoTSet = transitive_set()
AndroidDepsTSet = transitive_set()
ManifestTSet = transitive_set(args_projections = {"artifacts": _artifacts})
PrebuiltNativeLibraryDirTSet = transitive_set()
ResourceInfoTSet = transitive_set()

DepsInfo = record(
    name = TargetLabel,
    deps = list[TargetLabel],
)

AndroidPackageableInfo = provider(
    # @unsorted-dict-items
    fields = {
        "target_label": provider_field(typing.Any, default = None),  # "target_label"
        "build_config_infos": provider_field(typing.Any, default = None),  # ["AndroidBuildConfigInfoTSet", None]
        "deps": provider_field(typing.Any, default = None),  # ["AndroidDepsTSet", None]
        "manifests": provider_field(typing.Any, default = None),  # ["ManifestTSet", None]
        "prebuilt_native_library_dirs": provider_field(typing.Any, default = None),  # ["PrebuiltNativeLibraryDirTSet", None]
        "resource_infos": provider_field(typing.Any, default = None),  # ["AndroidResourceInfoTSet", None]
    },
)

RESOURCE_PRIORITY_NORMAL = "normal"
RESOURCE_PRIORITY_LOW = "low"

# Information about an `android_resource`
AndroidResourceInfo = provider(
    # @unsorted-dict-items
    fields = {
        # Target that produced this provider
        "raw_target": provider_field(typing.Any, default = None),  # TargetLabel
        # output of running `aapt2_compile` on the resources, if resources are present
        "aapt2_compile_output": provider_field(typing.Any, default = None),  # Artifact | None
        #  if False, then the "res" are not affected by the strings-as-assets resource filter
        "allow_strings_as_assets_resource_filtering": provider_field(typing.Any, default = None),  # bool
        # assets defined by this rule. May be empty
        "assets": provider_field(typing.Any, default = None),  # Artifact | None
        # manifest file used by the resources, if resources are present
        "manifest_file": provider_field(typing.Any, default = None),  # Artifact | None
        # the package specified by the android_resource rule itself
        "specified_r_dot_java_package": provider_field(typing.Any, default = None),  # str | None
        # package used for R.java, if resources are present
        "r_dot_java_package": provider_field(typing.Any, default = None),  # Artifact | None
        # resources defined by this rule. May be empty
        "res": provider_field(typing.Any, default = None),  # Artifact | None
        # priority of the resources, may be 'low' or 'normal'
        "res_priority": provider_field(typing.Any, default = None),  # str
        # symbols defined by the resources, if resources are present
        "text_symbols": provider_field(typing.Any, default = None),  # Artifact | None
    },
)

# `AndroidResourceInfos` that are exposed via `exported_deps`
ExportedAndroidResourceInfo = provider(
    fields = {
        "resource_infos": provider_field(typing.Any, default = None),  # ["AndroidResourceInfo"]
    },
)

DexFilesInfo = record(
    primary_dex = Artifact,
    primary_dex_class_names = [Artifact, None],
    root_module_secondary_dex_dirs = list[Artifact],
    non_root_module_secondary_dex_dirs = list[Artifact],
    secondary_dex_exopackage_info = [ExopackageDexInfo, None],
    proguard_text_files_path = [Artifact, None],
)

ExopackageInfo = record(
    secondary_dex_info = [ExopackageDexInfo, None],
    native_library_info = [ExopackageNativeInfo, None],
    resources_info = [ExopackageResourcesInfo, None],
)

AndroidLibraryIntellijInfo = provider(
    # @unsorted-dict-items
    doc = "Information about android library that is required for Intellij project generation",
    fields = {
        "android_resource_deps": provider_field(typing.Any, default = None),  # ["AndroidResourceInfo"]
        "dummy_r_dot_java": provider_field(typing.Any, default = None),  # ["artifact", None]
    },
)

def merge_android_packageable_info(
        label: Label,
        actions: AnalysisActions,
        deps: list[Dependency],
        build_config_info: [AndroidBuildConfigInfo, None] = None,
        manifest: [Artifact, None] = None,
        prebuilt_native_library_dir: [PrebuiltNativeLibraryDir, None] = None,
        resource_info: [AndroidResourceInfo, None] = None) -> AndroidPackageableInfo:
    android_packageable_deps = filter(None, [x.get(AndroidPackageableInfo) for x in deps])

    build_config_infos = _get_transitive_set(
        actions,
        filter(None, [dep.build_config_infos for dep in android_packageable_deps]),
        build_config_info,
        AndroidBuildConfigInfoTSet,
    )

    deps = _get_transitive_set(
        actions,
        filter(None, [dep.deps for dep in android_packageable_deps]),
        DepsInfo(
            name = label.raw_target(),
            deps = [dep.target_label for dep in android_packageable_deps],
        ),
        AndroidDepsTSet,
    )

    manifests = _get_transitive_set(
        actions,
        filter(None, [dep.manifests for dep in android_packageable_deps]),
        ManifestInfo(
            target_label = label.raw_target(),
            manifest = manifest,
        ) if manifest else None,
        ManifestTSet,
    )

    prebuilt_native_library_dirs = _get_transitive_set(
        actions,
        filter(None, [dep.prebuilt_native_library_dirs for dep in android_packageable_deps]),
        prebuilt_native_library_dir,
        PrebuiltNativeLibraryDirTSet,
    )

    resource_infos = _get_transitive_set(
        actions,
        filter(None, [dep.resource_infos for dep in android_packageable_deps]),
        resource_info,
        ResourceInfoTSet,
    )

    return AndroidPackageableInfo(
        target_label = label.raw_target(),
        build_config_infos = build_config_infos,
        deps = deps,
        manifests = manifests,
        prebuilt_native_library_dirs = prebuilt_native_library_dirs,
        resource_infos = resource_infos,
    )

def _get_transitive_set(
        actions: AnalysisActions,
        children: list[TransitiveSet],
        node: typing.Any,
        transitive_set_definition: TransitiveSetDefinition) -> [TransitiveSet, None]:
    kwargs = {}
    if children:
        kwargs["children"] = children
    if node:
        kwargs["value"] = node

    return actions.tset(transitive_set_definition, **kwargs) if kwargs else None

def merge_exported_android_resource_info(
        exported_deps: list[Dependency]) -> ExportedAndroidResourceInfo:
    exported_android_resource_infos = []
    for exported_dep in exported_deps:
        exported_resource_info = exported_dep.get(ExportedAndroidResourceInfo)
        if exported_resource_info:
            exported_android_resource_infos += exported_resource_info.resource_infos

        android_resource = exported_dep.get(AndroidResourceInfo)
        if android_resource:
            exported_android_resource_infos.append(android_resource)

    return ExportedAndroidResourceInfo(resource_infos = dedupe(exported_android_resource_infos))

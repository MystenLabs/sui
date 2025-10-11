# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import pathlib
import shutil
import zipfile
from tempfile import TemporaryDirectory

from java.tools import utils


CLASSES_JAR_FILE_NAME = "classes.jar"
ANNOTATIONS_ZIP_FILE_NAME = "annotations.zip"
LIBS_DIR_NAME = "libs"


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to unpack an aar for android_prebuilt_aar."
    )

    parser.add_argument(
        "--aar", type=pathlib.Path, required=True, help="a path to the aar to unpack"
    )
    parser.add_argument(
        "--manifest-path",
        type=pathlib.Path,
        required=True,
        help="a path to the manifest that is unpacked",
    )
    parser.add_argument(
        "--all-classes-jar-path",
        type=pathlib.Path,
        required=True,
        help="a path to the single output jar containing all the Java classes that this aar contains",
    )
    parser.add_argument(
        "--r-dot-txt-path",
        type=pathlib.Path,
        required=True,
        help="a path to the R.txt that is unpacked",
    )
    parser.add_argument(
        "--res-path",
        type=pathlib.Path,
        required=True,
        help="a path to the resources that are unpacked",
    )
    parser.add_argument(
        "--assets-path",
        type=pathlib.Path,
        required=True,
        help="a path to the assets that are unpacked",
    )
    parser.add_argument(
        "--jni-path",
        type=pathlib.Path,
        required=True,
        help="a path to the native lib directory that is unpacked",
    )
    parser.add_argument(
        "--annotation-jars-dir",
        type=pathlib.Path,
        required=True,
        help="a path to a directory where the external annotations.zip may be copied if present",
    )
    parser.add_argument(
        "--proguard-config-path",
        type=pathlib.Path,
        required=True,
        help="a path to the proguard config that is unpacked",
    )
    parser.add_argument(
        "--jar-builder-tool",
        type=str,
        required=True,
        help="tool for building jars",
    )

    return parser.parse_args()


def main():
    args = _parse_args()

    aar_path = args.aar
    manifest_path = args.manifest_path
    all_classes_path = args.all_classes_jar_path
    res_path = args.res_path
    assets_path = args.assets_path
    jni_path = args.jni_path
    r_dot_txt_path = args.r_dot_txt_path
    annotation_jars_dir = args.annotation_jars_dir
    proguard_config_path = args.proguard_config_path
    jar_builder_tool = args.jar_builder_tool

    with TemporaryDirectory() as temp_dir:
        unpack_dir = pathlib.Path(temp_dir)
        with zipfile.ZipFile(aar_path, "r") as aar_zip:
            aar_zip.extractall(unpack_dir)

        # If the zip file was built on e.g. Windows, then it might not have
        # correct permissions (which means we can't read any of the files), so
        # make sure we actually read everything here.
        utils.execute_command(["chmod", "-R", "+rX", unpack_dir])

        unpacked_manifest = unpack_dir / "AndroidManifest.xml"
        assert unpacked_manifest.exists()
        shutil.copyfile(unpacked_manifest, manifest_path)

        unpacked_res = unpack_dir / "res"
        if unpacked_res.exists():
            shutil.copytree(unpacked_res, res_path)
        else:
            res_path.mkdir()

        unpacked_assets = unpack_dir / "assets"
        if unpacked_assets.exists():
            shutil.copytree(unpacked_assets, assets_path)
        else:
            assets_path.mkdir()

        unpacked_jni = unpack_dir / "jni"
        if unpacked_jni.exists():
            shutil.copytree(unpacked_jni, jni_path)
        else:
            jni_path.mkdir()

        unpacked_r_dot_txt = unpack_dir / "R.txt"
        if unpacked_r_dot_txt.exists():
            shutil.copyfile(unpacked_r_dot_txt, r_dot_txt_path)
        else:
            r_dot_txt_path.touch()

        annotation_jars_dir.mkdir()
        annotations_zip_path = unpack_dir / ANNOTATIONS_ZIP_FILE_NAME
        if annotations_zip_path.exists():
            shutil.copy(annotations_zip_path, annotation_jars_dir)

        unpacked_proguard_config = unpack_dir / "proguard.txt"
        if unpacked_proguard_config.exists():
            shutil.copyfile(unpacked_proguard_config, proguard_config_path)
        else:
            proguard_config_path.touch()

        # Java .class files can exist at `classes.jar` or any jar file in /libs,
        # so combine them into a single `.jar` file.
        all_jars = []
        classes_jar = unpack_dir / CLASSES_JAR_FILE_NAME
        if classes_jar.exists():
            all_jars.append(classes_jar)

        libs_dir = unpack_dir / LIBS_DIR_NAME
        if libs_dir.exists():
            libs = [lib for lib in libs_dir.iterdir() if lib.suffix == ".jar"]
            all_jars += libs

        jars_list = unpack_dir / "jars_list.txt"
        with open(jars_list, "w") as f:
            f.write("\n".join([str(jar) for jar in all_jars]))

        combine_all_jars_cmd = utils.shlex_split(jar_builder_tool) + [
            "--entries-to-jar",
            jars_list,
            "--output",
            all_classes_path,
        ]

        utils.execute_command(combine_all_jars_cmd)


if __name__ == "__main__":
    main()

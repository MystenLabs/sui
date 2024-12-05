# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
from contextlib import ExitStack
from enum import Enum
from pathlib import Path

from .preprocess import preprocess
from .process import process


class _SubcommandName(str, Enum):
    preprocess = "preprocess"
    process = "process"


def _create_preprocess_subparser(subparsers):
    parser = subparsers.add_parser(
        _SubcommandName.preprocess.value,
        description="Sub-command to expand macro variables in parametrized Info.plist files. It's the Buck v2 equivalent of what `FindAndReplaceStep` and `InfoPlistSubstitution` do.",
    )
    parser.add_argument(
        "--input",
        metavar="<Input.plist>",
        type=Path,
        required=True,
        help="Path to the input which is a .plist file ",
    )
    parser.add_argument(
        "--output",
        metavar="<Output.plist>",
        type=Path,
        required=True,
        help="Path where the output, .plist with applied substitutions, should be written to",
    )
    parser.add_argument(
        "--product-name",
        metavar="<Product Name>",
        type=str,
        required=True,
        help="Product name, the value of `apple_bundle().product_name` attribute to be used in substitutions",
    )
    parser.add_argument(
        "--substitutions-json",
        metavar="<Substitutions JSON File>",
        type=Path,
        help="JSON file containing substitutions mapping",
    )


def _create_process_subparser(subparsers):
    parser = subparsers.add_parser(
        _SubcommandName.process.value,
        description="Sub-command to do the final processing of the Info.plist before it's copied to the application bundle. It's the Buck v2 equivalent of what `PlistProcessStep` does in v1.",
    )
    parser.add_argument(
        "--input",
        metavar="<Input.plist>",
        type=Path,
        required=True,
        help="Path to unprocessed .plist file",
    )
    parser.add_argument(
        "--override-input",
        metavar="<OverrideInput.plist>",
        type=Path,
        help="Path to the additional .plist file which should be merged into final result overriding keys present in unprocessed file or any other --additional-* argument.",
    )
    parser.add_argument(
        "--additional-keys",
        metavar="<AdditionalKeys.json>",
        type=Path,
        help="Path to .json file containing additional data which should be merged into the final result if keys are not yet present in unprocessed file.",
    )
    parser.add_argument(
        "--override-keys",
        metavar="<OverrideKeys.json>",
        type=Path,
        help="Path to .json file with additional data which should be merged into the final result overriding keys present in unprocessed file or any other --additional-* or --override-* argument.",
    )
    parser.add_argument(
        "--output",
        metavar="<Output.plist>",
        type=Path,
        required=True,
        help="Path where processed .plist file should be placed",
    )


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to process Info.plist file before it is placed into the bundle. It's the Buck v2 equivalent of what `AppleInfoPlist` build rule from v1 does."
    )
    subparsers = parser.add_subparsers(dest="subcommand_name")
    _create_preprocess_subparser(subparsers)
    _create_process_subparser(subparsers)
    return parser.parse_args()


def main():
    args = _parse_args()
    if args.subcommand_name == _SubcommandName.preprocess:
        with ExitStack() as stack:
            input_file = stack.enter_context(args.input.open(mode="r"))
            output_file = stack.enter_context(args.output.open(mode="w"))
            substitutions_json = (
                stack.enter_context(args.substitutions_json.open(mode="r"))
                if args.substitutions_json is not None
                else None
            )
            preprocess(input_file, output_file, substitutions_json, args.product_name)
    elif args.subcommand_name == _SubcommandName.process:
        with ExitStack() as stack:
            input_file = stack.enter_context(args.input.open(mode="rb"))
            output_file = stack.enter_context(args.output.open(mode="wb"))
            override_input = (
                stack.enter_context(args.override_input.open(mode="rb"))
                if args.override_input is not None
                else None
            )
            additional_keys = (
                stack.enter_context(args.additional_keys.open(mode="rb"))
                if args.additional_keys is not None
                else None
            )
            override_keys = (
                stack.enter_context(args.override_keys.open(mode="rb"))
                if args.override_keys is not None
                else None
            )
            process(
                input_file=input_file,
                output_file=output_file,
                override_input_file=override_input,
                additional_keys_file=additional_keys,
                override_keys_file=override_keys,
            )


if __name__ == "__main__":
    main()

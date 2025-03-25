# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import json
import plistlib
from typing import Any, Dict, IO, Optional

from apple.tools.plistlib_utils import detect_format_and_load

# Corresponding v1 code is contained in `com/facebook/buck/apple/PlistProcessStep.java`, `PlistProcessStep::execute` method.
def _merge_plist_dicts(
    source: Dict[str, Any], destination: Dict[str, Any], override: bool = False
) -> None:
    for key, value in source.items():
        if key not in destination:
            destination[key] = value
        elif isinstance(value, dict) and isinstance(destination[key], dict):
            destination[key].update(value)
        elif override:
            # override the value
            destination[key] = value


def process(
    input_file: IO,
    output_file: IO,
    override_input_file: Optional[IO] = None,
    additional_keys: Optional[Dict[str, Any]] = None,
    additional_keys_file: Optional[IO] = None,
    override_keys_file: Optional[IO] = None,
    output_format: plistlib.PlistFormat = plistlib.FMT_BINARY,
) -> None:
    root = detect_format_and_load(input_file)
    if override_input_file is not None:
        extra = detect_format_and_load(override_input_file)
        # Overriding here for backwards compatibility with v1,
        # see `PlistProcessStep::execute` implementation
        _merge_plist_dicts(source=extra, destination=root, override=True)
    if additional_keys is not None:
        _merge_plist_dicts(source=additional_keys, destination=root)
    if additional_keys_file is not None:
        additional_keys_from_file = json.load(additional_keys_file)
        _merge_plist_dicts(source=additional_keys_from_file, destination=root)
    if override_keys_file is not None:
        override_keys_from_file = json.load(override_keys_file)
        _merge_plist_dicts(
            source=override_keys_from_file, destination=root, override=True
        )
    plistlib.dump(root, output_file, fmt=output_format)

# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import json
import os
from dataclasses import dataclass
from io import TextIOBase
from pathlib import Path
from typing import Any, Dict, List, Optional

_METADATA_VERSION = 1


@dataclass
class _Item:
    path: Path
    digest: str


@dataclass
class _Metadata:
    version: int
    digests: List[_Item]


def _object_hook(dict: Dict[str, Any]) -> Any:
    if "version" in dict:
        return _Metadata(**dict)
    else:
        dict["path"] = Path(dict.pop("path"))
        return _Item(**dict)


def parse_action_metadata(data: TextIOBase) -> Optional[Dict[Path, str]]:
    """
    Returns:
        Mapping from project relative path to hash digest for every file present action metadata.
    """
    start_stream_position = data.tell()
    try:
        metadata = json.load(data, object_hook=_object_hook)
    except BaseException:
        data.seek(start_stream_position)
        version = json.load(data)["version"]
        if version != _METADATA_VERSION:
            raise RuntimeError(
                f"Expected metadata version to be `{_METADATA_VERSION}` got `{version}`."
            )
        else:
            raise
    return {item.path: item.digest for item in metadata.digests}


def action_metadata_if_present(
    environment_variable_key: str,
) -> Optional[Dict[Path, str]]:
    """
    Returns:
        Mapping from project relative path to hash digest for every file present action metadata.
    """
    environment_variable = os.getenv(environment_variable_key)
    if environment_variable is None:
        return None
    path = Path(environment_variable)
    if not path.exists():
        raise RuntimeError(
            "Expected file with action metadata to exist given related environment variable is set."
        )
    else:
        with path.open() as f:
            return parse_action_metadata(f)

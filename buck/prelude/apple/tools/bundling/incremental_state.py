# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import json
from dataclasses import dataclass
from io import TextIOBase
from pathlib import Path
from typing import Any, Dict, List, Optional

from apple.tools.code_signing.codesign_bundle import CodesignConfiguration

_VERSION = 3


@dataclass
class IncrementalStateItem:
    source: Path
    """
    Path relative to buck project
    """
    destination_relative_to_bundle: Path
    digest: Optional[str]
    """
    Required when the source file is not a symlink
    """
    resolved_symlink: Optional[Path]
    """
    Required when the source file is a symlink
    """


@dataclass
class IncrementalState:
    """
    Describes a bundle output from a previous run of this bundling script.
    """

    items: List[IncrementalStateItem]
    codesigned: bool
    codesign_configuration: CodesignConfiguration
    codesign_on_copy_paths: List[Path]
    codesign_identity: Optional[str]
    swift_stdlib_paths: List[Path]
    version: int = _VERSION


class IncrementalStateJSONEncoder(json.JSONEncoder):
    def default(self, o: Any) -> Any:
        if isinstance(o, IncrementalState):
            return {
                "items": [self.default(i) for i in o.items],
                "codesigned": o.codesigned,
                "codesign_configuration": o.codesign_configuration.value
                if o.codesign_configuration
                else None,
                "codesign_on_copy_paths": [str(p) for p in o.codesign_on_copy_paths],
                "codesign_identity": o.codesign_identity,
                "swift_stdlib_paths": [str(p) for p in o.swift_stdlib_paths],
                "version": o.version,
            }
        elif isinstance(o, IncrementalStateItem):
            result = {
                "source": str(o.source),
                "destination_relative_to_bundle": str(o.destination_relative_to_bundle),
            }
            if o.digest is not None:
                result["digest"] = o.digest
            if o.resolved_symlink is not None:
                result["resolved_symlink"] = str(o.resolved_symlink)
            return result
        else:
            return super().default(o)


def _object_hook(dict: Dict[str, Any]) -> Any:
    if "version" in dict:
        dict["codesign_on_copy_paths"] = [
            Path(p) for p in dict.pop("codesign_on_copy_paths")
        ]
        codesign_configuration = dict.pop("codesign_configuration")
        dict["codesign_configuration"] = (
            CodesignConfiguration(codesign_configuration)
            if codesign_configuration
            else None
        )
        dict["swift_stdlib_paths"] = [Path(p) for p in dict.pop("swift_stdlib_paths")]
        return IncrementalState(**dict)
    else:
        dict["source"] = Path(dict.pop("source"))
        dict["destination_relative_to_bundle"] = Path(
            dict.pop("destination_relative_to_bundle")
        )
        dict["digest"] = dict.pop("digest", None)
        resolved_symlink = dict.pop("resolved_symlink", None)
        dict["resolved_symlink"] = Path(resolved_symlink) if resolved_symlink else None
        return IncrementalStateItem(**dict)


def parse_incremental_state(data: TextIOBase) -> IncrementalState:
    start_stream_position = data.tell()
    try:
        incremental_state = json.load(data, object_hook=_object_hook)
    except BaseException:
        data.seek(start_stream_position)
        version = json.load(data)["version"]
        if version != _VERSION:
            raise RuntimeError(
                f"Expected incremental state version to be `{_VERSION}` got `{version}`."
            )
        else:
            raise
    return incremental_state

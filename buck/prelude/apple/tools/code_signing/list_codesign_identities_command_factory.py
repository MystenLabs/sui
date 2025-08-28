# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from __future__ import annotations

from abc import ABCMeta, abstractmethod
from typing import List


class IListCodesignIdentitiesCommandFactory(metaclass=ABCMeta):
    @abstractmethod
    def list_codesign_identities_command(self) -> List[str]:
        raise NotImplementedError


class ListCodesignIdentitiesCommandFactory(IListCodesignIdentitiesCommandFactory):
    _default_command = ["security", "find-identity", "-v", "-p", "codesigning"]

    def __init__(self, command: List[str]):
        self.command = command

    @classmethod
    def default(cls) -> ListCodesignIdentitiesCommandFactory:
        return cls(cls._default_command)

    @classmethod
    def override(cls, command: List[str]) -> ListCodesignIdentitiesCommandFactory:
        return cls(command)

    def list_codesign_identities_command(self) -> List[str]:
        return self.command

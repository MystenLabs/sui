# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from abc import ABCMeta, abstractmethod
from pathlib import Path
from typing import List, Union


class IReadProvisioningProfileCommandFactory(metaclass=ABCMeta):
    @abstractmethod
    def read_provisioning_profile_command(self, path: Path) -> List[Union[str, Path]]:
        raise NotImplementedError


class DefaultReadProvisioningProfileCommandFactory(
    IReadProvisioningProfileCommandFactory
):
    # See `DEFAULT_READ_COMMAND` in `AppleConfig.java` in Buck v1
    _command = [
        "openssl",
        "smime",
        "-inform",
        "der",
        "-verify",
        "-noverify",
        "-in",
    ]

    def read_provisioning_profile_command(self, path: Path) -> List[Union[str, Path]]:
        return DefaultReadProvisioningProfileCommandFactory._command + [path]

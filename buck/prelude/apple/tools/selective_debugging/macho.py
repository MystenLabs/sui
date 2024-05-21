# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from dataclasses import dataclass

MH_MAGIC = 0xFEEDFACE
MH_CIGAM = 0xCEFAEDFE
MH_MAGIC_64 = 0xFEEDFACF
MH_CIGAM_64 = 0xCFFAEDFE

LC_CODE_SIGNATURE = 0x1D
LC_SEGMENT_64 = 0x19
LC_SYMTAB = 0x02

N_OSO = 0x66


class MachO:
    def __str__(self):
        props = {}
        for k, v in self.__dict__.items():
            props[k] = hex(v)
        return str(props)


@dataclass
class MachOHeader(MachO):
    magic: int
    cpu_type: int
    cpu_subtype: int
    file_type: int
    n_cmds: int
    size_of_cmds: int
    flags: int
    reserved: int

    @property
    def is_valid(self):
        return self.magic in (MH_CIGAM_64, MH_MAGIC_64)


@dataclass
class LoadCommand(MachO):
    cmd: int
    cmd_size: int


@dataclass
class LinkEditCommand(LoadCommand):
    segment_name: bytes
    VM_addr: int
    VM_size: int
    file_offset: int
    file_size: int
    maximum_VM_protection: int
    initial_VM_protection: int
    n_sections: int
    flags: int


@dataclass
class SymtabCommand(LoadCommand):
    cmd: int
    cmd_size: int
    symtab_offset: int
    n_symbols: int
    strtab_offset: int
    strtab_size: int


@dataclass
class Symbol(MachO):
    strtab_index: int
    sym_type: int
    section_index: int
    desc: int
    value: int

#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import re
import struct
import subprocess
import tempfile
from dataclasses import dataclass
from typing import List, Optional, Tuple


@dataclass(frozen=True)
class SegmentMap:
    segname: str
    vmaddr: int
    vmsize: int
    fileoff: int
    filesize: int

    def contains(self, va: int) -> bool:
        return self.vmaddr <= va < (self.vmaddr + self.vmsize) and self.filesize > 0

    def va_to_fileoff(self, va: int) -> Optional[int]:
        if not self.contains(va):
            return None
        delta = va - self.vmaddr
        if delta >= self.filesize:
            # VA points into zero-fill area (e.g., __bss); can't read from file
            return None
        return self.fileoff + delta


@dataclass(frozen=True)
class ReachPoint:
    assertion_type: str  # "reachable" or "sometimes"
    loc: str
    msg: str


def run(cmd: List[str]) -> str:
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\n{p.stderr.strip()}")
    return p.stdout


def thin_if_universal(path: str, arch: str) -> Tuple[str, Optional[tempfile.NamedTemporaryFile]]:
    """
    Returns (path_to_use, temp_file_handle_or_None).
    If universal, creates a thinned temp file for the requested arch.
    """
    info = run(["/usr/bin/lipo", "-info", path]).strip()
    # Examples:
    # "Non-fat file: ... is architecture: arm64"
    # "Architectures in the fat file: ... are: x86_64 arm64"
    if "Architectures in the fat file" not in info:
        return path, None

    tmp = tempfile.NamedTemporaryFile(prefix="reach_thin_", delete=False)
    tmp.close()
    run(["/usr/bin/lipo", path, "-thin", arch, "-output", tmp.name])
    return tmp.name, tmp  # caller unlinks tmp.name


def parse_otool_segments_and_section(otool_l_output: str) -> Tuple[List[SegmentMap], Tuple[int, int, int]]:
    """
    Parse `otool -l` output to:
      - gather all segment_64 vmaddr/fileoff mappings
      - locate section __reach_points and return (sect_addr, sect_offset, sect_size)

    Returns: (segments, (sect_addr, sect_offset, sect_size))
    Raises if section not found.
    """
    segs: List[SegmentMap] = []
    sect_addr = sect_offset = sect_size = None

    # We'll parse in a streaming-ish way: detect LC_SEGMENT_64 blocks and within them detect sections.
    lines = otool_l_output.splitlines()
    i = 0

    # Helpers to grab "key value" lines like "vmaddr 0x...."
    keyval_re = re.compile(r"^\s*([a-zA-Z_]+)\s+(0x[0-9a-fA-F]+|\d+)\s*$")
    strval_re = re.compile(r"^\s*([a-zA-Z_]+)\s+(.+?)\s*$")

    while i < len(lines):
        line = lines[i]
        if line.strip() == "cmd LC_SEGMENT_64":
            # Parse segment header fields until we hit either "Section" or next "Load command"
            segname = None
            vmaddr = vmsize = fileoff = filesize = None

            # segment header is a "segment_command_64" block
            j = i + 1
            while j < len(lines):
                l = lines[j]
                if l.strip().startswith("Section"):
                    break
                if l.strip().startswith("Load command"):
                    break
                m = strval_re.match(l)
                if m:
                    k, v = m.group(1), m.group(2).strip()
                    if k == "segname":
                        segname = v
                    else:
                        m2 = keyval_re.match(l)
                        if m2:
                            k2, v2 = m2.group(1), m2.group(2)
                            if k2 in ("vmaddr", "vmsize", "fileoff", "filesize"):
                                val = int(v2, 0)
                                if k2 == "vmaddr":
                                    vmaddr = val
                                elif k2 == "vmsize":
                                    vmsize = val
                                elif k2 == "fileoff":
                                    fileoff = val
                                elif k2 == "filesize":
                                    filesize = val
                j += 1

            if segname and vmaddr is not None and vmsize is not None and fileoff is not None and filesize is not None:
                segs.append(SegmentMap(segname=segname, vmaddr=vmaddr, vmsize=vmsize, fileoff=fileoff, filesize=filesize))

            # Now parse any section blocks inside this segment until next "Load command"
            # Each section is introduced by a line "Section" then key/value lines including sectname, segname, addr, size, offset
            while j < len(lines) and not lines[j].strip().startswith("Load command"):
                if lines[j].strip().startswith("Section"):
                    sectname = sec_segname = None
                    addr = size = offset = None
                    j += 1
                    while j < len(lines):
                        l = lines[j]
                        if l.strip().startswith("Section") or l.strip().startswith("Load command"):
                            break
                        m = strval_re.match(l)
                        if m:
                            k, v = m.group(1), m.group(2).strip()
                            if k == "sectname":
                                sectname = v
                            elif k == "segname":
                                sec_segname = v
                            else:
                                m2 = keyval_re.match(l)
                                if m2:
                                    k2, v2 = m2.group(1), m2.group(2)
                                    if k2 == "addr":
                                        addr = int(v2, 0)
                                    elif k2 == "size":
                                        size = int(v2, 0)
                                    elif k2 == "offset":
                                        offset = int(v2, 0)
                        j += 1

                    if sectname == "__reach_points" and offset is not None and size is not None and addr is not None:
                        sect_addr, sect_offset, sect_size = addr, offset, size

                    continue
                j += 1

            i = j
            continue

        i += 1

    if sect_addr is None:
        raise RuntimeError("Could not find section __reach_points in otool output (is it linked into this binary?)")

    return segs, (sect_addr, sect_offset, sect_size)


def va_to_fileoff(segs: List[SegmentMap], va: int) -> Optional[int]:
    for s in segs:
        fo = s.va_to_fileoff(va)
        if fo is not None:
            return fo
    return None


def read_string_with_len(blob: bytes, start: int, length: int) -> str:
    """Read a string of known length from the blob (Rust str slices know their length)."""
    if start < 0 or start >= len(blob):
        raise ValueError(f"String file offset {start} out of range (size={len(blob)})")
    if start + length > len(blob):
        raise ValueError(f"String extends past end of file: offset {start} + length {length} > {len(blob)}")
    return blob[start:start + length].decode("utf-8", errors="replace")


def extract_reach_points(macho_path: str) -> List[ReachPoint]:
    otool_out = run(["/usr/bin/otool", "-l", macho_path])
    segs, (sect_addr, sect_offset, sect_size) = parse_otool_segments_and_section(otool_out)

    with open(macho_path, "rb") as f:
        blob = f.read()

    if sect_offset + sect_size > len(blob):
        raise RuntimeError("Section offset/size out of file bounds")

    section = blob[sect_offset: sect_offset + sect_size]

    # ReachableAssertion layout (three Rust &str fat pointers):
    #   assertion_type: ptr + len = 16 bytes
    #   loc: ptr + len = 16 bytes
    #   msg: ptr + len = 16 bytes
    # Total: 48 bytes per entry
    entry_sz = 48
    if (len(section) % entry_sz) != 0:
        raise RuntimeError(
            f"__reach_points size {len(section)} is not a multiple of {entry_sz}; "
            "does your ReachPoint layout match (three Rust &str fat pointers)?"
        )

    points: List[ReachPoint] = []
    for idx in range(0, len(section), entry_sz):
        type_ptr, type_len, loc_ptr, loc_len, msg_ptr, msg_len = struct.unpack_from("<QQQQQQ", section, idx)

        # Resolve virtual addresses to file offsets
        type_off = va_to_fileoff(segs, type_ptr)
        loc_off = va_to_fileoff(segs, loc_ptr)
        msg_off = va_to_fileoff(segs, msg_ptr)

        if type_off is None:
            raise RuntimeError(f"Could not map type_ptr VA=0x{type_ptr:x} to file offset (entry #{idx//entry_sz})")
        if loc_off is None:
            raise RuntimeError(f"Could not map loc_ptr VA=0x{loc_ptr:x} to file offset (entry #{idx//entry_sz})")
        if msg_off is None:
            raise RuntimeError(f"Could not map msg_ptr VA=0x{msg_ptr:x} to file offset (entry #{idx//entry_sz})")

        assertion_type = read_string_with_len(blob, type_off, type_len)
        loc = read_string_with_len(blob, loc_off, loc_len)
        msg = read_string_with_len(blob, msg_off, msg_len)

        points.append(ReachPoint(assertion_type=assertion_type, loc=loc, msg=msg))

    return points


def main() -> None:
    ap = argparse.ArgumentParser(description="Extract ReachPoints from a macOS Mach-O binary.")
    ap.add_argument("binary", help="Path to test binary")
    ap.add_argument("--arch", choices=["arm64", "x86_64"], default="arm64",
                    help="If binary is universal, pick which arch slice to inspect (default: arm64)")
    args = ap.parse_args()

    path = args.binary
    tmp_handle = None

    try:
        path_to_use, tmp_handle = thin_if_universal(path, args.arch)
        pts = extract_reach_points(path_to_use)
        for p in pts:
            print(f"{p.assertion_type}\t{p.loc}\t{p.msg}")
    finally:
        if tmp_handle is not None:
            try:
                os.unlink(tmp_handle.name)
            except OSError:
                pass


if __name__ == "__main__":
    main()

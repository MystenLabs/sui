# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import lldb

# = LLDB Frame Sizes =
#
# LLDB Utility to print the current backtrace with estimated stack
# frame sizes (useful for figuring out which frames are contributing
# most to a stack overflow).
#
# == Usage ==
#
#     (lldb) command script import ./sui/scripts/lldb_frame_sizes
#     Loaded "frame-sizes" command.
#     (lldb) ...
#     
#     Process XXXXX stopped
#     ...
#     (lldb) frame-sizes

def frame_sizes(debugger, command, result, internal_dict):
    """Estimates the sizes of stack frames in the current backtrace.

    Prints the function called in each stackframe along with an estimate of its
    frame size, based on stack pointers and frame pointer.

    This function assumes that stacks always grow down, and that the base/frame
    pointer is always available on a frame, which may not be true on all
    platforms or at all optimization levels, but works well for debug Apple
    ARM64 builds."""

    thread = debugger.GetSelectedTarget().GetProcess().GetSelectedThread()
    if len(thread) == 0:
        return

    # Stacks grow down (into lower addresses), so the stack pointer at
    # the top of the stack should be the lowest address we see
    frame_lo = thread.GetFrameAtIndex(0).GetSP()

    for frame in iter(thread):
        frame_hi = frame.GetFP()
        frame_sz = frame_hi - frame_lo
        frame_lo = frame_hi

        line_entry = frame.GetLineEntry()
        file_spec = line_entry.GetFileSpec()
        print(
            "{:>10}B {} ({}:{}:{})".format(
                frame_sz,
                frame.GetDisplayFunctionName(),
                file_spec.GetFilename(),
                line_entry.GetLine(),
                line_entry.GetColumn(),
            ),
            file=result,
        )

def __lldb_init_module(debugger, internal_dict):
    debugger.HandleCommand(
        'command script add -f lldb_frame_sizes.frame_sizes frame-sizes',
    )
    print('Loaded "frame-sizes" command.')

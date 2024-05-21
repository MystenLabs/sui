# Haskell Language Server integration

This integration allows loading `haskell_binary` and `haskell_library` targets
on Haskell Language Server. This is accomplished via a BXL script that is
used to drive a hie-bios "bios" cradle.

# Usage

To print the list of GHC flags and targets for a Haskell source file:

  buck2 bxl prelude//haskell/ide/ide.bxl -- --bios true --file <repo_relative_path_to_source_file>

To integrate with hie_bios, copy `hie.yaml` to your repo root

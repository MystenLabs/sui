# hmaptool

This tool was copied from llvm-project. See https://github.com/llvm/llvm-project/blob/main/clang/utils/hmaptool/hmaptool

## About

Header maps are binary files used by Xcode, which are used to map
header names or paths to other locations. Clang has support for
those since its inception, but there's not a lot of header map
testing around.

Since it's a binary format, testing becomes pretty much brittle
and its hard to even know what's inside if you don't have the
appropriate tools.

Add a python based tool that allows creating and dumping header
maps based on a json description of those. While here, rewrite
tests to use the tool and remove the binary files from the tree.

This tool was initially written by Daniel Dunbar.

Thanks to Stella Stamenova for helping make this work on Windows.

---
id: tools
title: Move Tools
custom_edit_url: https://github.com/move-language/move/edit/main/language/tools/README.md
---

# Summary

Move has a number of tools associated with it. This directory contains all,
or almost all of them. The following crates in this directory are libraries
that are used by the [`move-cli`](./move-cli) `package` subcommand:

* `move-bytecode-viewer`
* `move-disassembler`
* `move-explain`
* `move-unit-test`
* `move-package`
* `move-coverage`

In this sense each of these crates defines the core logic for a specific
package command, e.g., how to run and report unit tests, or collect and
display test coverage information. However, the Move CLI is responsible for
stitching these commands together, e.g., when running a move unit test the
Move CLI is responsible for first making sure the package was built in
`test` mode ( using the `move-package` library), collecting the test plan
to feed to the `move-unit-test` library, and returning a non-zero error
code if a test fails.

Generally, if you want to see how various tools interact with each other,
or how a normal Move user would interact with these tools, you should first
look at the Move CLI (specifically the `package` subdirectory/command) as
that is responsible for stitching everything together. If you are looking
for where the logic for a specific tool is defined, this is most likely in
the specific crate for that tool (e.g., if you want to see how TUIs are
handled for the `move-bytecode-viewer` that's defined in the
`move-bytecode-viewer` crate, and not the `move-cli` crate).

Some of the crates mentioned above are also binaries at the moment, however
they should all be able to be made libaries only, with the possible
exception of the `move-coverage` crate. The primary reason for this, is
that this tool can collect and report test coverage statistics across
multiple packages, and multiple runs over a package. This functionality is
important if you have a large functional test suite such as Diem's and want
to gather coverage information across all of them.

The `move-resource-viewer`, and `read-write-set` similarly are library
crates that are used by and exposed by the Move CLI, but not through the
`package` subcommand.

The `move-bytecode-utils` crates holds general
utilities for working with Move bytecode, e.g., computing the dependency
order for modules.

# Move Version 1.5

Version 1.5 of Move includes a new package system, a number of bug fixes, and other improvements.

## Highlights

* Packages: The Move package system is a new feature to allow Move programmers to
more easily re-use code and share it across projects.
More details are available in the package [change description](changes/7-packages.md).
* The Move Prover has extensive changes related to global invariants and specification
variables, along with many other enhancements and bug fixes.

## Move Language

[Named address](changes/5-named-addresses.md)
values are now expected to be specified in packages rather than
in the source code. The address values are then passed on the command line to the
compiler, with the expectation that developers will no longer invoke the compiler
directly but will instead build via the package system. Thus, named address
declarations in Move can no longer specify the address values
([#8880](https://github.com/diem/diem/pull/8880)).

## Compiler

* The Move compiler now makes a distinction between errors and non-fatal warnings,
so that it can report warning messages without causing a compilation to fail
([#8881](https://github.com/diem/diem/pull/8881)).
The parser is also a little bit better about continuing after certain kinds of errors
([#9175](https://github.com/diem/diem/pull/9175)).
* Performance: Removed an unnecessary copy
of the source code in memory to speed up build times by about 5%
([#8893](https://github.com/diem/diem/pull/8893))
and used string interning in the compiler to further reduce both build times
and memory use by another 30%
([#8874](https://github.com/diem/diem/pull/8874)).
* Checking for phantom types: the compiler now reports warnings for non-phantom
parameters that are either unused or used in a phantom position
([#8740](https://github.com/diem/diem/pull/8740)).
* Attributes: The compiler now reports better error messages for attributes, especially
for "known" attributes that are expected to be used in certain ways
([#9249](https://github.com/diem/diem/pull/9249)).

## Prover

* Global invariants: This release of the Move Prover includes extensive changes
related to global invariants. In previous releases, verification may have been
skipped for global invariants quantified over a generic memory and then evaluated
in a generic Move function. The solution provides more holistic support for type parameters
in the Move Prover. In particular, specifications for generic global invariants
previously written as `invariant forall t: type: ...` should now be written as
`invariant<T> ...` to express that the invariant applies to a generic type `T`.
Another important change is the addition of an invariant suspension scheme which
allows the verification of a global invariant to be deferred to a different
location (instead of right after the relevant bytecode). These schemas include
`disable_invariants_in_body` which postpones the evaluation to the end of the
function and `delegate_invariants_to_caller` which lifts the evaluation to the
caller side. This feature allows verification of more (or stronger) global
invariants as the caller side has more context to evaluate the
invariant. Internally, the bytecode transformation pipeline is updated such
that all global invariants related to a function are instrumented at
the appropriate locations. The monomorphization pass is updated to
instantiate type parameters on both the global invariant side and function
side.
* Specification variables (a.k.a., ghost variables): Specification variable
declarations can now optionally include an
initializer: for example, `global spec_var: num = 1`.
Specification variables can also now be updated in
a function spec block, for example: `update spec_var = spec_var + 1`.
The new implementation of specification variables maps them to
regular global "ghost" memory. Access to specification variables is mapped to
accessing this memory (at address `@0`), so that memory usage and global
invariant analysis can be applied uniformly to specification variables.
* Added new `--unconditional-abort-as-inconsistency` flag to treat
functions that do not return (i.e., abort unconditionally) as inconsistency violations
([#8861](https://github.com/diem/diem/pull/8861)).
* Added new `--ignore-pragma-opaque-when-possible` flag to ignore the `opaque` pragma
when possible
([#8843](https://github.com/diem/diem/pull/8843))
and a related `--ignore-pragma-opaque-internal-only` flag that only applies to internal
functions
([#8862](https://github.com/diem/diem/pull/8862)).
* Added the `is_signer` native specification function to the Signer module
in the standard library, along with support for that function in the Prover
([#8868](https://github.com/diem/diem/pull/8868)).
* Loop invariants: Added new `invariant` syntax which can be used in loop
headers to specify invariant conditions for those loops
([#8890](https://github.com/diem/diem/pull/8890) and
 [#8945](https://github.com/diem/diem/pull/8945)).
* The `--dump-bytecode` command line option now emits the bytecode dumps
in the parent directory of the output and uses a new naming convention for
the output files
([#9052](https://github.com/diem/diem/pull/9052)).
* The documentation generator has been extended to show the backward call graph
in addition to the forward call graph
([#9239](https://github.com/diem/diem/pull/9239)).
* The `spec_address_of` native specification function has been replaced by the
`address_of` Move function, which can also be used in specifications
([#9261](https://github.com/diem/diem/pull/9261)).

**Fixed bugs:**

* Crash in `move_model::ty::Type::replace`
([#9155](https://github.com/diem/diem/issues/9155)).
* The `choose` operator was handled incorrectly
([#8865](https://github.com/diem/diem/pull/8865)).
* Variable scope issue with shadowing of let-bound names in schemas
([#8854](https://github.com/diem/diem/issues/8854)).
* Uninterpreted functions should not have an `inline` attribute
([#8995](https://github.com/diem/diem/issues/8995)).
* The `--generate-only` flag was ignored
([#9092](https://github.com/diem/diem/pull/9092)).
* Boogie name resolution error: "use of undeclared function"
([#9156](https://github.com/diem/diem/issues/9156)).

## Standard Library

An experimental module for capability-based access control has been added
to the "nursery" area of the standard library
([#9305](https://github.com/diem/diem/pull/9305)).

## Documentation

The Move language documentation has been updated for both named addresses
([#9195](https://github.com/diem/diem/pull/9195))
and phantom types
([#9263](https://github.com/diem/diem/pull/9263)) and
 [#9339](https://github.com/diem/diem/pull/9339)),
as well as for the new Move packages
([#9241](https://github.com/diem/diem/pull/9241)).

## Miscellaneous

* For internal testing within the Move project, this release introduces a new
"transactional-tests" infrastructure (see the
`testing-infra/transactional-test-runner` directory). Many existing tests have
been migrated to use this and we will continue that migration over time.
Note that this testing framework is intended for internal use:
most testing of Move code should continue to use Move unit tests.
* The Move IR compiler, which is now only intended for internal testing purposes,
has some significant updates in this release. This tool is not intended for
external use so there is no documentation of the new IR syntax.
* The `MoveStorage` trait has been renamed to `MoveResolver` and moved to the
`move-core-types` crate so that it can be used without pulling in the VM as a
dependency
([#8886](https://github.com/diem/diem/pull/8886)).
* The `MoveStruct` type in `move-core-types` has been generalized to an enum that
can also specify the field names
([#8901](https://github.com/diem/diem/pull/8901)).
* The compiler's `AddressBytes` struct has been renamed to `NumericalAddress`
and changed into a wrapper around `AccountAddress`
([#9282](https://github.com/diem/diem/pull/9282)).
* Added a "generate struct-layouts" sandbox command to the Move CLI to dump
struct layouts in YAML format
([#9073](https://github.com/diem/diem/pull/9073)).

**Fixed bugs:**

* Fixed disassembler crash for scripts without source mappings
([#9005](https://github.com/diem/diem/pull/9005)).
* Fixed a problem with unit tests where an abort from native function
was not reported as an error
([#9143](https://github.com/diem/diem/pull/9143)).


# Move Version 1.4

Version 1.4 of Move (released along with Diem Core version 1.4) includes named addresses,
phantom type parameters, a new version of the bytecode format, and a number of bug fixes
and other improvements.

## Highlights

* Move Language Enhancements: This version of Move adds support for two new language features,
described in more detail in separate change descriptions:
    * [Named Addresses](changes/5-named-addresses.md): This allows names to be used in
      place of numerical values in any spot where addresses are used. (Some aspects of this
      feature were already included in Move version 1.3.)
    * [Phantom Type Parameters](changes/6-phantom-type-params.md): Type parameters for generic
      structs can now be declared `phantom` when they are not used for anything except
      compile-time type checking. This avoids the need for spurious abilities to satisfy the
      type checker.
* Version 3 of the Move bytecode format: The bytecode format has been changed to support
phantom type parameters. The Move VM still reads and processes older versions of the Move
bytecode, but new bytecode files will require the new Move VM version.

## Compiler

* Error messages from the compiler have been significantly revised, after updating the compiler
to use a recent version of the `codespan-reporting` crate
([#8812](https://github.com/diem/diem/pull/8812)).
* Fixed a report of memory leaks in the compiler by adding an internal pool of symbols
and using it to record source file names
([#8742](https://github.com/diem/diem/pull/8742)).
* Improved compiler performance by about 60% by rewriting the code for handling scoped aliases
([#8804](https://github.com/diem/diem/pull/8804)).

## Prover

* Set up a new lab to compare cvc5 with z3 in benchmarks
([#8732](https://github.com/diem/diem/pull/8732)).
* Improved error message if the Boogie command cannot be found
([#8778](https://github.com/diem/diem/pull/8778)).
* Fixed an inconsistency in global invariant processing that was exposed by using
the `disable_invariants_in_body` pragma
([#8840](https://github.com/diem/diem/pull/8840)).
Verifying global invariants remains an active area of development so there may
still be some related issues in this release.

## VM

* Added VM support for publishing multiple modules in a single transaction
([#8555](https://github.com/diem/diem/pull/8555)).
This allows publishing a set of interdependent modules that are verified and link-checked
together.
* Improved logging of errors when deserializing Move modules
([#8681](https://github.com/diem/diem/pull/8681)).
* Fixed performance problems when loading and verifying friend modules. In addition to
caching the results of deserializing and verifying modules, the loader has been
significantly refactored to be more robust and to improve its internal APIs
([#8832](https://github.com/diem/diem/pull/8832)).

## Command Line Interpreter (CLI)

* The `move compile` command now includes the Move standard library by default
([#8679](https://github.com/diem/diem/pull/8679)).
* Split out the Diem-specific part of the CLI to a new `df-cli` tool
([#8615](https://github.com/diem/diem/pull/8615))
and refactored the Move CLI so that `df-cli` (or other clients) can
extend it with new subcommands
([#8764](https://github.com/diem/diem/pull/8764)).

## Miscellaneous

* Updated the Move language book to use abilities
([#8582](https://github.com/diem/diem/pull/8582)).
The documentation still needs more updates to catch up with the latest features.
* Finished the process of removing Cargo dependencies on Diem crates,
continuing our effort to make Move usable apart from Diem.
* Refactored various command line tools to share common code in a new
`move-command-line-common` crate
([#8680](https://github.com/diem/diem/pull/8680)).
* Enhanced the Move bytecode disassembler to print the bytecode version
([#8690](https://github.com/diem/diem/pull/8690)).
* Removed the deprecated `CompiledScript::into_module` method
([#8655](https://github.com/diem/diem/pull/8655)).
* Removed the `CompiledModuleMut` and `CompiledScriptMut` types
([#8667](https://github.com/diem/diem/pull/8667))
along with the `into_inner`, `as_inner`, and `freeze` methods from the
`CompiledModule` and `CompiledScript` types
([#8712](https://github.com/diem/diem/pull/8712)).


# Move Version 1.3

Version 1.3 of Move (released along with Diem Core version 1.3) introduces some syntax changes
to the Move language so that you may need to update Move source code when moving to this release.
The bytecode format remains the same as in version 1.2.

## Highlights

The main highlight of this release is a new language feature for unit testing.
This provides an easy way to test individual functions and features in Move.
More details are available in the unit testing [change description](changes/4-unit-testing.md).

## Move Language

In addition to the new unit testing feature, this release includes a few other changes to the
Move language:

* Added new module address syntax, e.g., `module 0x1::M`, to specify the address of a module
from within Move code
([#7915](https://github.com/diem/diem/pull/7915)).
This replaces the compiler's `--sender` option to specify the address on the command line.

* The syntax for an address value is changed to `@` followed by a number
([#8285](https://github.com/diem/diem/pull/8285)).
Previously an account address value was specified
as a hexadecimal value with an `0x` prefix, and hexadecimal values could not be
used as ordinary integer numbers. With this change, addresses and numbers can be
specified as either decimal and hexadecimal values, and the `@` prefix distinguishes
the address values.

* Introduced a general syntax for attributes in Move
([#8169](https://github.com/diem/diem/pull/8169)).
Move attributes are based on the Rust attribute syntax, which is in turn based on
the standards found in ECMA-334 and ECMA-335. Attributes can currently be attached to
address blocks, modules, scripts, and any module top level member. They are currently
used for unit testing, and other attributes may be defined in the future.

## Compiler

* Removed the compiler's `--sender` option. Instead of specifying the address on the command line,
you can use the new module address syntax in the Move code
([#7915](https://github.com/diem/diem/pull/7915)).
* Fixed crashes during internal testing with a precompiled standard library
when error messages reference the precompiled files
([#8344](https://github.com/diem/diem/pull/8344)).

## Prover

The syntax for specifications in Move is still in development and is
documented separately from the rest of the language. This release includes a
number of changes for Move specifications:

* Extended and renamed builtin functions
* New `let` binding semantics (`let x = E` and `let post y = E`)
* Support for axiom for constraining uninterpreted specification functions
* New `choose x where p` and `choose min i where p` expression forms
* New invariant syntax (`module M { invariant p; }`) for global invariants
* New syntax for function and struct specifications (`spec f` instead of `spec fun f`)
* New syntax for specification modules which can be put into separate files
* Removed `succeeds_if`
* Removed `invariant module`
* Removed `type<T>()` expression

In addition to the specification changes, the Move Prover has been improved with
bug fixes and some larger changes, including:

* Overhauled handling of global invariants
* Changed to perform monomorphization of generics in the Prover backend and memory model,
which has helped the Prover run faster and avoid timeouts.

## Standard Library

* Added a `BitVector` module
([#8315](https://github.com/diem/diem/pull/8315)).
* Added a `Vault` module for capability-based secure storage
([#8396](https://github.com/diem/diem/pull/8396)).

## VM

* Renamed `gas_schedule::CostStrategy` to `GasStatus` and cleaned up some of its APIs
([#7797](https://github.com/diem/diem/pull/7797)).
* Encapsulated both the `ChangeSet` and `AccountChangeSet` types so that their fields must be
accessed by API functions, which also enforce a new invariant that the `AccountChangeSet` is
not empty
([#8288](https://github.com/diem/diem/pull/8288)).
* Added a missing bounds check in the bytecode verifier for the `self_module_handle_idx` field
([#8389](https://github.com/diem/diem/pull/8389)).

## Miscellaneous

* Fixed the Move disassembler to work correctly with abilities
([#8128](https://github.com/diem/diem/pull/8128)).
* Renamed the `vm` Rust crate to `move-binary-format`,
which is a much better description of its contents.
([#8161](https://github.com/diem/diem/pull/8161)).
* Removed a number of dependencies on Diem crates,
continuing our effort to make Move usable apart from Diem.
* Added an `ident_str!` macro to create const `IdentStr` values
([#8300](https://github.com/diem/diem/pull/8300)).
* Refactored the `MoveResource` trait to add a separate `MoveStructType` trait
([#8346](https://github.com/diem/diem/pull/8346)).
* Added back the Move language documentation files, now in the `mdBook` format
([#8450](https://github.com/diem/diem/pull/8450)).
* Fixed the Move script binding generator so that the generated code is valid
when there are no transaction scripts or script functions
([#8465](https://github.com/diem/diem/pull/8465)).


# Move Version 1.2

Version 1.2 of Move (released along with Diem Core version 1.2) includes several new language features, a new version of the bytecode format, significant improvements to the Move Prover, and numerous bug fixes.

## Highlights

* Move Language Enhancements: This version of Move adds support for three new language features. Each of these is described in more detail in separate change descriptions.
    * [Friend Visibility](changes/1-friend-visibility.md): a new visibility modifier that allows a function to be called only by a set of declared `friend` modules.
    * [Script Visibility](changes/2-script-visibility.md): a new visibility modifier that allows a function to be called only from a transaction or another script function.
    * [Abilities](changes/3-abilities.md): a generalization of the existing `resource`/`struct` distinction to enable more fine-grained control over the operations allowed on a record value.
* Version 2 of the Move bytecode format: The bytecode format has been changed to support the new features. The Move VM still reads and processes older versions of the Move bytecode, but new bytecode files will require the new Move VM version.
* Move Prover: verification speed improvements of 2x and more via new internal architecture.

## VM

This release includes several changes and enhancements:

* Arguments to Move functions are now specified as BCS-serialized values ([#7170](https://github.com/diem/diem/pull/7170)) and the VM also returns serialized values ([#7599](https://github.com/diem/diem/pull/7599)). The VM’s `execute_function` API now returns the serialized return values ([#7671](https://github.com/diem/diem/pull/7671)).
* The VM’s file format deserializer now supports versioning so that it can seamlessly read multiple versions of Move bytecode files ([#7323](https://github.com/diem/diem/pull/7323)).
* The VM’s module publishing API now allows republishing an existing module, as long as the updated module is backward compatible with the previous version ([#7143](https://github.com/diem/diem/pull/7143)). This includes a new bytecode verifier check for module updates that introduce cyclic dependencies ([#7234](https://github.com/diem/diem/pull/7234)) and related checks for cyclic dependencies when building and loading the standard library ([#7475](https://github.com/diem/diem/pull/7475)).
* A new  `InternalGasUnits` type has been introduced to distinguish the unscaled units within the VM from the scaled `GasUnits` type ([#7448](https://github.com/diem/diem/pull/7448)).

**Fixed bugs:**

* Creating a normalized struct type now correctly uses the module handle associated with the `StructHandleIndex` rather than the module containing the declaration ([#7321](https://github.com/diem/diem/pull/7321)).
* The expected output files for internal tests no longer used colons in the file names, for the sake of file systems that do not support that ([#7770](https://github.com/diem/diem/issues/7770)).
* The `parse_type_tag` function can now handle struct names containing underscores ([#7151](https://github.com/diem/diem/issues/7151)).
* Missing signature checks for the `MoveToGeneric`, `ImmBorrowFieldGeneric`, and `MutBorrowFieldGeneric`  instructions have been added to the bytecode verifier ([#7752](https://github.com/diem/diem/pull/7752)).

## Standard Library

To make it easier to use Move for projects besides Diem, we are working toward separating the parts of Move that are specific to Diem. There is much more to do, but in this release, the standard library has been separated into two parts: `move-stdlib` ([#7633](https://github.com/diem/diem/pull/7633)) and `diem-framework` ([#7529](https://github.com/diem/diem/pull/7529)).

## Compiler

Besides adding support for the new language features mentioned above, the compiler in this release includes a number of fixes and usability enhancements:

* Attempting to use a global storage builtin, e.g., `move_to`, in a script context will no longer crash the compiler ([#4577](https://github.com/diem/diem/issues/4577)).
* Hex strings with an odd number of characters are no longer accepted by the compiler ([#6577](https://github.com/diem/diem/issues/6577)).
* A `let` binding with a name starting with an underscore, e.g., `_x`, can now be used later in the code: the underscore prefix merely disables the compiler diagnostic about unused locals ([#6786](https://github.com/diem/diem/pull/6786)).
* Fixed a compiler crash when a `break` is used outside of a loop ([#7560](https://github.com/diem/diem/issues/7560)).
* Added a missing check for recursive types when binding to a local variable, which fixed a compiler crash with a stack overflow ([#7562](https://github.com/diem/diem/issues/7562)).
* Fixed a compiler crash for an infinite loop with unreachable exits ([#7568](https://github.com/diem/diem/issues/7568)).
* Fixed a compiler crash due to an unassigned local used in an equality comparison ([#7569](https://github.com/diem/diem/issues/7569)).
* Fixed a compiler crash due to borrowing a divergent expression ([#7570](https://github.com/diem/diem/issues/7570)).
* Fixed a compiler crash due to a missing constraint for references in the type checker ([#7573](https://github.com/diem/diem/issues/7573)).
* Fixed a compiler crash related to expressions with short-circuiting ([#7574](https://github.com/diem/diem/issues/7574)).
* Fixed an incorrect code generation bug that could occur when a function parameter is assigned a new value exactly once in the function ([#7370](https://github.com/diem/diem/pull/7370)).
* Fixed the bytecode source map mapping from local names to indexes so that function parameters go before locals ([#7371](https://github.com/diem/diem/pull/7371)).
* Fixed a compiler crash when a struct is assigned without specifying its fields ([#7385](https://github.com/diem/diem/issues/7385)).
* Fixed a compiler crash when attempting to put a `spec` block inside a `spec` context ([#7387](https://github.com/diem/diem/issues/7387)).
* An integer literal value that is too large for its declared type will no longer cause a compiler crash ([#7388](https://github.com/diem/diem/issues/7388)).
* Fixed a compiler crash caused by incorrect number of type parameters in pack/unpack expressions ([#7401](https://github.com/diem/diem/pull/7401)).
* Module names and module members are now restricted from starting with underscores (‘_’) , which also avoids a crash ([#7572](https://github.com/diem/diem/issues/7572)).
* Prover specifications are now included in the compiler’s dependency ordering calculation ([#7960](https://github.com/diem/diem/pull/7960)).
* Modified the compiler optimization to remove fall-through jumps so that loop headers are not coalesced, which improves the prover’s ability to handle loop specifications ([#8049](https://github.com/diem/diem/pull/8049)).

## Command Line Interpreter (CLI)

The Move CLI has been enhanced in several ways:

* The CLI now supports safe module republishing with checks for breaking changes ([#6753](https://github.com/diem/diem/pull/6753)).
* Added a new `doctor` command to detect inconsistencies in storage ([#6971](https://github.com/diem/diem/pull/6971), [#7010](https://github.com/diem/diem/pull/7010), and [#7013](https://github.com/diem/diem/pull/7013)).
* The `publish` command’s `—-dry-run` option has been removed ([#6957](https://github.com/diem/diem/pull/6957)). Use the equivalent "check" command instead.
* The `test` command has a new `--create` option to create test scaffolding ([#6969](https://github.com/diem/diem/pull/6969)).
* The verbose output with the `-v` option now includes the number of bytes written ([#7757](https://github.com/diem/diem/pull/7757)).

## Other Tools

* Created a new bytecode-to-source explorer tool for Move ([#7508](https://github.com/diem/diem/pull/7508)).
* The resource viewer can now be better used to traverse data structures because the fields of `AnnotatedMoveStruct` are no longer private and `AnnotatedMoveValue::Vector` preserves the type information for its elements ([#7166](https://github.com/diem/diem/pull/7166)).
* The `diem-writeset-generator` and `diem-transaction-replay` tools have been significantly enhanced to support the process of upgrading the Diem Framework.

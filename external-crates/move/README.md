---
id: move-language
title: Move Language
custom_edit_url: https://github.com/move-language/move/edit/main/language/README.md
---

Move is a new programming language developed to provide a safe and programmable foundation for smart contract development.

## Overview

The Move language directory consists of three distinct parts: the Move bytecode language, the Move intermediate representation (IR), and the Move source language.

- The Move bytecode language defines programs published to the blockchain. It has a static type system that guarantees the absence of certain critical errors, including the prevention of duplicating certain values. Relevant crates include:
  - [move-binary-format](crates/move-binary-format/) defines the binary format for Move bytecode.
  - [move-bytecode-verifier](crates/move-bytecode-verifier/) provides the static safety checks for Move bytecode.
  - [move-vm-types](crates/move-vm-types/) provides a shared utility crate types used by the Move VM and adapting layers.
  - [move-vm-runtime](crates/move-vm-runtime/) provides the runtime for the Move VM, used for executing Move programs.

- The Move IR is a low-level intermediate representation that closely mirrors Move bytecode. What mainly differentiates it from the bytecode is that names are used as opposed to indexes into pools/tables. Relevant crates include:
  - [move-ir-types](crates/move-ir-types/) defines the IRs Rust types, notably the AST.
  - [move-ir](crates/move-ir-to-bytecode/) compiles the IR to the bytecode.

- The Move source language is a high-level language that compiles to Move bytecode. It is designed to be a familiar and ergonomic language for developers that provides minimal abstractions over the Move bytecode. Relevant crates include:
  - [move-compiler](crates/move-compiler/) defines the source language and all compilation steps up to the Move IR.
  - [move-analyzer](crates/move-analyzer/) provides the IDE integration for the source language.
  - [move-package](crates/move-package/) defines the package system used for organizing Move source code.
  - [move-cli](crates/move-cli/) provides a standard implementation for command-line utilities, such as building Move binary files and running tests.

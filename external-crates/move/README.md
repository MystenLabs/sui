---
id: move-language
title: Move Language
custom_edit_url: https://github.com/move-language/move/edit/main/language/README.md
---


Move is a new programming language developed to provide a safe and programmable foundation for the Diem Blockchain.

## Overview

The Move language directory consists of four main parts:

- [virtual machine](move-vm/) (VM) &mdash; contains the bytecode format, a bytecode interpreter, and infrastructure for executing a block of transactions. This directory also contains the infrastructure to generate the genesis block.

- [bytecode verifier](move-bytecode-verifier/) &mdash; contains a static analysis tool for rejecting invalid Move bytecode. The virtual machine runs the bytecode verifier on any new Move code it encounters before executing it. The compiler runs the bytecode verifier on its output and surfaces the errors to the programmer.

- [move-compiler](move-compiler/) &mdash; contains the Move source language compiler.

- [standard library](move-stdlib/) &mdash; contains the standard library transaction scripts.

## Exploring the Move language

- You can find many small Move examples in the [tests](move-compiler/tests/move_check/) directory. The easiest way to experiment with Move is to create a new test in this directory and run it with `cargo test`.

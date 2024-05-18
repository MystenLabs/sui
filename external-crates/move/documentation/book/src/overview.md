---
id: overview
title: Overview
sidebar_label: Move
---

Move is a next generation language for secure asset programming. Its primary use case is in
blockchain environments, where Move programs are used to construct state changes. Move allows
developers to write programs that flexibly manage and transfer assets, while providing the security
and protections against attacks on those assets.

### Start Here

<CardsWrapper>
  <SimpleTextCard
    icon="img/introduction-to-move.svg"
    iconDark="img/introduction-to-move-dark.svg"
    overlay="Understand Move’s background, current status and architecture"
    title="Introduction"
    to="/docs/move/move-introduction"
  />
  <SimpleTextCard
    icon="img/modules.svg"
    iconDark="img/modulesdark.svg"
    overlay="Understand Move Modules"
    title="Modules "
    to="/docs/move/move-modules"
  />
</CardsWrapper>

### Primitive Types

<CardsWrapper>
  <SimpleTextCard
    icon="img/integers-bool.svg"
    iconDark="img/integers-bool-dark.svg"
    overlay="Move supports three unsigned integer types: u8, u64, and u128"
    title="Integers"
    to="/docs/move/move-integers"
  />
  <SimpleTextCard
    icon="img/integers-bool.svg"
    iconDark="img/integers-bool-dark.svg"
    overlay="bool is Move's primitive type for boolean true and false values."
    title="Bool"
    to="/docs/move/move-bool"
  />
  <SimpleTextCard
    icon="img/address.svg"
    iconDark="img/address-dark.svg"
    overlay="address is a built-in type in Move that is used to represent locations in storage"
    title="Address"
    to="/docs/move/move-address"
  />
  <SimpleTextCard
    icon="img/vector.svg"
    iconDark="img/vector-dark.svg"
    overlay="vector<T> is the only primitive collection type provided by Move"
    title="Vector"
    to="/docs/move/move-vector"
  />
  <SimpleTextCard
    icon="img/move-references.svg"
    iconDark="img/move-references-dark.svg"
    overlay="Move has two types of references: immutable & and mutable &mut"
    title="References"
    to="/docs/move/move-references"
  />
  <SimpleTextCard
    icon="img/tuples.svg"
    iconDark="img/tuples-dark.svg"
    overlay="In order to support multiple return values, Move has tuple-like expressions. We can consider unit() to be an empty tuple"
    title="Tuples and Unit"
    to="/docs/move/move-tuples-and-unit"
  />
</CardsWrapper>

### Basic Concepts

<CardsWrapper>
  <SimpleTextCard
    icon="img/local-variables-and-scopes.svg"
    iconDark="img/local-variables-and-scopes-dark.svg"
    overlay="Local variables in Move are lexically (statically) scoped"
    title="Local Variables and Scopes"
    to="/docs/move/move-variables"
  />
  <SimpleTextCard
    icon="img/abort-and-return.svg"
    iconDark="img/abort-and-return-dark.svg"
    overlay="return and abort are two control flow constructs that end execution, one for the current function and one for the entire transaction"
    title="Abort & Assert"
    to="/docs/move/move-abort-and-assert"
  />
  <SimpleTextCard
    icon="img/conditionals.svg"
    iconDark="img/conditionals-dark.svg"
    overlay="An if expression specifies that some code should only be evaluated if a certain condition is true"
    title="Conditionals"
    to="/docs/move/move-conditionals"
  />
  <SimpleTextCard
    icon="img/loops.svg"
    iconDark="img/loops-dark.svg"
    overlay="Move offers two constructs for looping: while and loop"
    title="While and Loop"
    to="/docs/move/move-while-and-loop"
  />
  <SimpleTextCard
    icon="img/functions.svg"
    iconDark="img/functions-dark.svg"
    overlay="Function syntax in Move is shared between module functions and script functions"
    title="Functions"
    to="/docs/move/move-functions"
  />
  <SimpleTextCard
    icon="img/structs.svg"
    iconDark="img/structsdark.svg"
    overlay="A struct is a user-defined data structure containing typed fields."
    title="Structs"
    to="/docs/move/move-structs"
  />
  <SimpleTextCard
    icon="img/constants.svg"
    iconDark="img/constants-dark.svg"
    overlay="Constants are a way of giving a name to shared, static values inside of a module or script"
    title="Constants"
    to="/docs/move/move-constants"
  />
  <SimpleTextCard
    icon="img/generics.svg"
    iconDark="img/generics-dark.svg"
    overlay="Generics can be used to define functions and structs over different input data types"
    title="Generics"
    to="/docs/move/move-generics"
  />
  <SimpleTextCard
    icon="img/equality.svg"
    iconDark="img/equality-dark.svg"
    overlay="Move supports two equality operations == and !="
    title="Equality"
    to="/docs/move/move-equality"
  />
  <SimpleTextCard
    icon="img/uses-and-aliases.svg"
    iconDark="img/uses-and-aliases-dark.svg"
    overlay="The use syntax can be used to create aliases to members in other modules"
    title="Uses & Aliases"
    to="/docs/move/move-uses-and-aliases"
  />
</CardsWrapper>

### Reference

<CardsWrapper>
  <SimpleTextCard
    icon="img/coding-conventions.svg"
    iconDark="img/coding-conventions-dark.svg"
    overlay="There are basic coding conventions when writing Move code"
    title="Coding Conventions"
    to="/docs/move/move-coding-conventions"
  />
</CardsWrapper>

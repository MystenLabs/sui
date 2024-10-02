# DEPRECATED: Friends

NOTE: this feature has been superceded by [`public(package)`](./functions.md#visibility).

The `friend` syntax was used to declare modules that are trusted by the current module. A trusted
module is allowed to call any function defined in the current module that have the `public(friend)`
visibility. For details on function visibilities, refer to the _Visibility_ section in
[Functions](./functions.md).

## Friend declaration

A module can declare other modules as friends via friend declaration statements, in the format of

- `friend <address::name>` — friend declaration using fully qualified module name like the example
  below, or

  ```move
  module 0x42::a {
      friend 0x42::b;
  }
  ```

- `friend <module-name-alias>` — friend declaration using a module name alias, where the module
  alias is introduced via the `use` statement.

  ```move
  module 0x42::a {
      use 0x42::b;
      friend b;
  }
  ```

A module may have multiple friend declarations, and the union of all the friend modules forms the
friend list. In the example below, both `0x42::B` and `0x42::C` are considered as friends of
`0x42::A`.

```move
module 0x42::a {

    friend 0x42::b;
    friend 0x42::c;
}
```

Unlike `use` statements, `friend` can only be declared in the module scope and not in the expression
block scope. `friend` declarations may be located anywhere a top-level construct (e.g., `use`,
`function`, `struct`, etc.) is allowed. However, for readability, it is advised to place friend
declarations near the beginning of the module definition.

### Friend declaration rules

Friend declarations are subject to the following rules:

- A module cannot declare itself as a friend.

  ```move=
  module 0x42::m { friend Self; // ERROR! }
  //                      ^^^^ Cannot declare the module itself as a friend

  module 0x43::m { friend 0x43::M; // ERROR! }
  //                      ^^^^^^^ Cannot declare the module itself as a friend
  ```

- Friend modules must be known by the compiler

  ```move=
  module 0x42::m { friend 0x42::nonexistent; // ERROR! }
  //                      ^^^^^^^^^^^^^^^^^ Unbound module '0x42::nonexistent'
  ```

- Friend modules must be within the same account address.

  ```move=
  module 0x42::m {}

  module 0x42::n { friend 0x42::m; // ERROR! }
  //                      ^^^^^^^ Cannot declare modules out of the current address as a friend
  ```

- Friends relationships cannot create cyclic module dependencies.

  Cycles are not allowed in the friend relationships, e.g., the relation `0x2::a` friends `0x2::b`
  friends `0x2::c` friends `0x2::a` is not allowed. More generally, declaring a friend module adds a
  dependency upon the current module to the friend module (because the purpose is for the friend to
  call functions in the current module). If that friend module is already used, either directly or
  transitively, a cycle of dependencies would be created.

  ```move=
  module 0x2::a {
      use 0x2::c;
      friend 0x2::b;

      public fun a() {
          c::c()
      }
  }

  module 0x2::b {
      friend 0x2::c; // ERROR!
  //         ^^^^^^ This friend relationship creates a dependency cycle: '0x2::b' is a friend of '0x2::a' uses '0x2::c' is a friend of '0x2::b'
  }

  module 0x2::c {
      public fun c() {}
  }
  ```

- The friend list for a module cannot contain duplicates.

  ```move=
  module 0x42::a {}

  module 0x42::m {
      use 0x42::a as aliased_a;
      friend 0x42::A;
      friend aliased_a; // ERROR!
  //         ^^^^^^^^^ Duplicate friend declaration '0x42::a'. Friend declarations in a module must be unique
  }
  ```

An example of how to structure your code to include another user-level package as a dependency.

The main things to pay attention to is to make sure that the dependency name (in this example
DepPackage in main_package/Move.toml file's `[dependency]` section) is the same as the name of the
package (in dep_package/Move.toml file's `[package]` section) are exactly the same

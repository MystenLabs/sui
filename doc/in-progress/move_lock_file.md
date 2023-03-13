# `Move.lock` File for Move Packages

When you build a Move package, the process creates a `Move.lock` file at the root of your package. The file acts as a communication layer between the Move compiler and other tools, like chain-specific command line interfaces and third-party package managers. The `Move.lock` file contains information about your package, including its dependencies, that aids operations like verification of source code against on-chain packages and package manager compatability.    

Although the `Move.lock` file is text-based, it's not intended for you to edit the file directly. Processes on the toolchain, like the Move compiler, access and edit the file to read and append relevant information. You also must not move the file from the root, as it needs to be at the same level as your `Move.toml` manifest. 

If you are using source control for your package, make sure the `Move.lock` file is checked in to your repository. This ensures every build of your package are exact replicas.   

## Why `Move.lock`

Although the `Move.lock` file serves many purposes, it's principle functions are on-chain source verification and integration of third-party package managers.   

### On-chain source verification

`Move.lock` records the toolchain version and flags passed during package compilation so the compiler can replicate the process. This provides confirmation that the package found at a given address on chain originated from a specific source package. Having this data in the lock file means you don't have to manually provide this information, which would be time consuming and prone to error.

### Integrating third-party package managers

Third-party package managers use the `Move.lock` file to integrate with the Sui network.

## Format

The `Move.lock` file is a text-based file similar to the `Move.tonl` package manifest file. 



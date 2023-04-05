This folder contains a Move package consisting of core Move modules defining functionalities needed by
any Diem-based blockchains. To start your own Diem-based blockchain, you can create a Move package that
depends on this core package and write your own wrapper modules for the account module as well as the
on-chain configuration modules. [`experimental` folder](../experimental) provides an example of such
Move package.

Next we briefly describe how to write the wrapper modules and how Move's powerful type system can provide flexibility
as well as safety for your chain.

You will need to write wrapper modules for the following core modules:
- Account
- CoreGenesis
- DiemConsensusConfig
- DiemSystem
- DiemVersion
- DiemVMConfig
- ParallelExecutionConfig
- ValidatorConfig
- ValidatorOperatorConfig


Every module in this list contains the following resources and functions:
- A chain marker resource with a `phantom T` type parameter
- An initialization function that has to be called during chain-specific genesis and publishes a chain marker resource under
  `@CoreResources` address as well as any relevant configuration resources
- At least one setter function that requires a `Cap<T>` as parameter and modifies the configuration resource(s).
  And at the beginning of the function we check the existence of the chain marker resource.

#### Why is the chain marker resource needed?

We want to provide the safety guarantee that only the authorized wrapper module can define access
control policies for the resources. Therefore, we need to make sure that only the wrapper module can call the public
setter functions. However, we are designing for wrapper modules that don't even exist right now, so we have to somehow
"register" the wrapper module at the chain-specific genesis. The solution to this question is the chain marker resource.
This resource has a phantom type parameter that should be instantiated with a type owned by the wrapper module such as
`ExperimentalVersion` resource. Each setter function requires a capability parameter instantiated with this type. Since
the acquisition of this capability requires a value of this witness type and this type is owned by the wrapper module,
we know for sure that only the wrapper module can decide who or which module is allowed to modify the configurations.

#### How to write wrapper modules?

Read `DiemVersion.move` and then `ExperimentalVersion.move` to get the idea.

[]{#sec:model label="sec:model"}

In this section, we expand on the informal description of the Sui
programming model from Section
[\[sec:move\]](#sec:move){reference-type="ref" reference="sec:move"} by
presenting detailed semantic definitions. The previous section showed
examples of Move source code; here we define the structure of Move
bytecode. Developers write, test, and formally
verify [@DBLP:journals/corr/abs-2110-08362; @DBLP:conf/cav/ZhongCQGBPZBD20]
Move source code locally, then compile it to Move bytecode before
publishing it to the blockchain. Any Move bytecode be published on-chain
must pass through a *bytecode
verifier*[@move_white; @DBLP:journals/corr/abs-2004-05106] to ensure
that it satisfies key properties such as type, memory, and resource
safety.

As mentioned in Section [\[sec:move\]](#sec:move){reference-type="ref"
reference="sec:move"}, Move is a platform-agnostic language which can be
adapted to fit specific needs of different systems without forking the
core language. In the following description, we define both concepts
from core Move language (denoted in black text) and Sui-specific
features extending the core Move language (denoted with text).

## Modules {#sec:mod-pkg}

::: {#tab:mod-pkg}
  ------------------------ --------------------------------------------------------------------------------------------------------------
  $\Module =$              $\ModuleName \times$
                           $(\StructName \pfun \StructDecl) \times$
                           $(\ProcName \pfun \ProcDecl) \times \suimove{\ProcDecl}$
  $\mathsf{\GenParam} =$   $\seq{\mathsf{Ability}}$
  $\StructDecl =$          $(\FieldName \pfun \StoreableType) \times$
                           $\seq{\mathsf{Ability}} \times \seq{\mathsf{\GenParam}}$
  $\ProcDecl$ =            $\seq{\Type} \seq{\Type} \times \seq{\Instruction} \times \seq{\mathsf{\GenParam}}$
  $\Instruction=$          $\suimove{\mathsf{TransferToAddr}} ~|~ \suimove{\mathsf{TransferToObj}} ~|~ \suimove{\mathsf{ShareMut}} ~|~$
                           $\suimove{\mathsf{ShareImmut}} ~|~ \ldots$
  ------------------------ --------------------------------------------------------------------------------------------------------------

  : Module
:::

Move code is organized into *modules* whose structure is defined in
Table [1](#tab:mod-pkg){reference-type="ref" reference="tab:mod-pkg"}. A
module consists of a collection of named *struct* declarations and a
collection of named *function* declarations (examples of these
declaration are provided in
Section [\[sec:move-overview\]](#sec:move-overview){reference-type="ref"
reference="sec:move-overview"}). A module also contains a special
function declaration serving as the module *initializer*. This function
is invoked exactly once at the time the module is published on-chain.

A struct declaration is a collection of named fields, where a field name
is mapped to a storeable type. Its declaration also includes an optional
list of abilities (see
Section [2](#tab:types-abilities){reference-type="ref"
reference="tab:types-abilities"} for a description of storeable types
and abilities). A struct declaration may also include a list of *generic
parameters* with ability constraints, in which case we call it a
*generic struct* declaration, for example . A generic parameter
represents a type to be used when declaring struct fields -- it is
unknown at the time of struct declaration, with a *concrete* type
provided when the struct is instantiated (i.e., as struct value is
created).

A function declaration includes a list of parameter types, a list of
return types, and a list of instructions forming the function's body. A
function declaration may also include a list of generic parameters with
ability constraints, in which case we call it a *generic function*
declaration, for example . Similarly to struct declarations, a generic
parameter represents a type unknown at function declaration time, but
which is nevertheless used when declaring function parameters, return
values and a function body (concrete type is provided when a function is
called).

Instructions that can appear in a function body include all ordinary
Move instructions with the exception of global storage instructions
(e.g., , , ). See [@DBLP:journals/corr/abs-2110-05043] for a complete
list of core Move's instructions and their semantics. In Sui persistent
storage is supported via Sui's global object pool rather than the
account-based global storage of core Move.

There are four Sui-specific object operations. Each of these operations
changes the ownership metadata of the object (see Section
[0.3](#sec:objects){reference-type="ref" reference="sec:objects"}) and
returns it to the global object pool. Most simply, a Sui object can be
transferred to the address of a Sui end-user. An object can also be
transferred to another *parent* object--this operation requires the
caller to supply a mutable reference to the parent object in addition to
the child object. An object can be mutably *shared* so it can be
read/written by anyone in the Sui system. Finally, an object can be
immutably shared so it can be read by anyone in the Sui system, but not
written by anyone.

The ability to distinguish between different kinds of ownership is a
unique feature of Sui. In other blockchain platforms we are aware of,
every contract and object is mutably shared. As we will explain in
Section [\[sec:system\]](#sec:system){reference-type="ref"
reference="sec:system"}, Sui leverages this information for parallel
transaction execution (for all transactions) and parallel agreement (for
transactions involving objects without shared mutability).

## Types and Abilities

::: {#tab:types-abilities}
  ---------------------- ---------------------------------------------------------------------------------------------------
  $\GroundType =$        $\set{\texttt{address}, \suimove{\texttt{id}}, \texttt{bool}, \texttt{u8}, \texttt{u64}, \ldots}$
  $\StructType =$        $\ModuleName \times \StructName \times$
                         $\seq{\StoreableType}$
  $\StoreableType =$     $\GroundType \uplus \StructType \uplus$
                         $\TypeArg \uplus \VecType$
  $\VecType =$           $\StoreableType$
  $\TypeArg =$           $\mathbb{N}$
  $\MutabilityQual =$    $\set{\texttt{mut}, \texttt{immut}}$
  $\ReferenceType =$     $\StoreableType \times \MutabilityQual$
  $\Type =$              $\mathsf{ReferenceType} \uplus \StoreableType$
  $\mathsf{Ability} =$   $\set{\texttt{key}, \texttt{store}, \texttt{copy}, \texttt{drop}}$
  ---------------------- ---------------------------------------------------------------------------------------------------

  : Types and Abilities
:::

A Move program manipulates both data stored in Sui global object pool
and transient data created when the Move program executes. Both objects
and transient data are Move *values* at the language level. However, not
all values are created equal -- they may have different properties and
different structure as prescribed by their types.

The types used in Move are defined in
Table [2](#tab:types-abilities){reference-type="ref"
reference="tab:types-abilities"}. Move supports many of the same
*primitive types* supported in other programming languages, such as a
boolean type or unsigned integer types of various sizes. In addition,
core Move has an type representing an end-user in the system that is
also used to identify the sender of a transaction and (in Sui) the owner
of an object. Finally, Sui defines an type representing an identity of a
Sui object-- see Section [0.3](#sec:objects){reference-type="ref"
reference="sec:objects"} for details.

A *struct type* describes an instance (i.e., a value) of a struct
declared in a given module (see
Section [0.1](#sec:mod-pkg){reference-type="ref"
reference="sec:mod-pkg"} for information on struct declarations). A
struct type representing a generic struct declaration (i.e., *generic
struct* type) includes a list of *storeable types* -- this list is the
counterpart of the generic parameter list in the struct declaration. A
storeable type can be either a *concrete type* (a primitive or a struct)
or a *generic type*. We call such types storeable because they can
appear as fields of structs and in objects stored persistently on-chain,
whereas reference types cannot.

For example, the struct type is a generic struct type parameterized with
a concrete (primitive) storeable type -- this kind of type can be used
to create a struct instance (i.e.,value). On the other hand, the same
generic struct type can be parameterized with a generic type (e.g., )
coming from a generic parameter of the enclosing struct or function
declaration -- this kind of type can be used to declare struct fields,
function params, etc. Structurally, a generic type is an integer index
(defined as $\mathbb{N}$ in Table [5](#tab:txn){reference-type="ref"
reference="tab:txn"}) into the list of generic parameters in the
enclosing struct or function declaration.

A *vector type* in Move describes a variable length collection of
homogenous values. A Move vector can only contain storeable types, and
it is also a storeable type itself.

A Move program can operate directly on values or access them indirectly
via references. A *reference type* includes both the storeable type
referenced and a *mutability qualifier* used to determine (and enforce)
whether a value of a given type can be read and written (`mut`) or only
read (`immut`). Consequently, the most general form of a Move value type
( in Table [2](#tab:types-abilities){reference-type="ref"
reference="tab:types-abilities"}) can be either a storeable type or a
reference type.

Finally, *abilities* in Move control what actions are permissible for
values of a given type, such as whether a value of a given type can be
copied (duplicated). Abilities constraint struct declarations and
generic type parameters. The Move bytecode verifier is responsible for
ensuring that sensitive operations like copies can only be performed on
types with the corresponding ability.

## Objects and Ownership {#sec:objects}

::: {#tab:objects-ownership}
  -------------------------- -----------------------------------------------------------------------------------
  $\transactiondigest =$     $Com(\transaction)$
  $\objectid =$              $Com(\transactiondigest \times \mathbb{N})$
  $\mathsf{SingleOwner} =$   $\AccountAddress \uplus \objectid$
  $\mathsf{Shared} =$        $\set{\texttt{shared\_mut}, \texttt{shared\_immut}}$
  $\mathsf{Ownership} =$     $\mathsf{SingleOwner} \uplus \mathsf{Shared}$
  $\mathsf{StructObj} =$     $\StructType \times \Struct$
  $\mathsf{ObjContents} =$   $\mathsf{StructObj} \uplus \mathsf{Package}$
  $\Object =$                $\mathsf{ObjContents} \times \objectid \times \mathsf{Ownership} \times \Version$
  -------------------------- -----------------------------------------------------------------------------------

  : Objects and Ownership
:::

Each Sui object has a globally unique identifier ($\objectid$ in
Table [3](#tab:objects-ownership){reference-type="ref"
reference="tab:objects-ownership"}) that serves as the persistent
identity of the object as it flows between owners and into and out of
other objects. This ID is assigned to the object by the transaction that
creates it. An object ID is created by applying a collision-resistant
hash function to the contents of the current transaction and to a
counter recording how many objects the transaction has created. A
transaction (and thus its digest) is guaranteed to be unique due to
constraints on the input objects of the transaction, as we will explain
subsequently.

In addition to an ID, each object carries metadata about its ownership.
An object is either uniquely owned by an address or another object,
shared with write/read permissions, or shared with only read
permissions. The ownership of an object determines whether and how a
transaction can use it as an input. Broadly, a uniquely owned object can
only be used in a transaction initiated by its owner or including its
parent object as an input, whereas a shared object can be used by any
transaction, but only with the specified mutability permissions. See
Section [\[sec:owners\]](#sec:owners){reference-type="ref"
reference="sec:owners"} for a full explanation.

There are two types of objects: package code objects, and struct data
objects. A package object contains of a list of modules. A struct object
contains a Move struct value and the Move type of that value. The
contents of an object may change, but its ID, object type (package vs
struct) and Move struct type are immutable. This ensures that objects
are strongly typed and have a persistent identity.

Finally, an object contains a version. Freshly created objects have
version 0, and an object's version is incremented each time a
transaction takes the object as an input.

## Addresses and Authenticators {#sec:address}

::: {#tab:addr-authenticator}
  --------------------- --------------------------------------------------------------------
  $\Authenticator =$    $\mathsf{Ed25519PubKey} \uplus \mathsf{ECDSAPubKey} \uplus \ldots$
  $\AccountAddress =$   $Com(\Authenticator)$
  --------------------- --------------------------------------------------------------------

  : Addresses and Authenticators
:::

An address is the persistent identity of a Sui end-user (although note
that a single user can have an arbitrary number of addresses). To
transfer an object to another user, the sender must know the address of
the recipient.

As we will discuss shortly, a Sui transaction must contain the address
of the user sending (i.e., initiating) the transaction and an
*authenticator* whose digest matches the address. The separation between
addresses and authenticators enables *cryptographic agility*. An
authenticator can be a public key from any signature scheme, even if the
schemes use different key lengths (e.g., to support post-quantum
signatures). In addition, an authenticator need not be a single public
key--it could also be (e.g.) a K-of-N multisig key.

## Transactions

::: {#tab:txn}
  ------------------------ ----------------------------------------------------------------------------------------------------------------------
  $\ObjRef=$               $\objectid \times \Version \times Com(\Object)$
  $\textsf{CallTarget}=$   $\ObjRef \times \ModuleName \times \ProcName$
  $\textsf{CallArg}=$      $\ObjRef \uplus \objectid \uplus \GroundType$
  $\mathsf{Package} =$     $\seq{\Module}$
  $\textsf{Publish}=$      $\mathsf{Package} \times \seq{\ObjRef}$
  $\textsf{Call} =$        $\textsf{CallTarget} \times \seq{\StoreableType} \times \seq{\mathsf{CallArg}}$
  $\mathsf{GasInfo} =$     $\ObjRef \times \textsf{MaxGas} \times \textsf{BaseFee} \times \textsf{Tip}$
  $\mathsf{Tx} =$          $(\textsf{Call} \uplus \textsf{Publish}) \times \mathsf{GasInfo} \times \mathsf{Addr} \times \mathsf{Authenticator}$
  ------------------------ ----------------------------------------------------------------------------------------------------------------------

  : Transactions
:::

Sui has two different transaction types: publishing a new Move package,
and calling a previously published Move package. A publish transaction
contains a *package*--a set of modules that will be published together
as a single object, as well as the dependencies of all the modules in
this package (encoded as a list of object references that must refer to
already-published package objects). To execute a publish transaction,
the Sui runtime will run the Move bytecode verifier on each package,
link the package against its dependencies, and run the module
initializer of each module. Module initializers are useful for
bootstrapping the initial state of an application implemented by the
package.

A call transaction's most important arguments are object inputs. Object
arguments are either specified via an object reference (for single-owner
and shared immutable objects) or an object ID (for shared mutable
objects). An object reference consists of an object ID, an object
version, and the hash of the object value. The Sui runtime will resolve
both object ID's and object references to object values stored in the
global object pool. For object references, the runtime will check the
version of the reference against the version of the object in the pool,
as well as checking that the reference's hash matches the pool object.
This ensures that the runtime's view of the object matches the
transaction sender's view of the object.

In addition, a call transaction accepts type arguments and pure value
arguments. Type arguments instantiate generic type parameters of the
entrypoint function to be invoked (e.g., if the entrypoint function is ,
the generic type parameter could be instantiated with the type argument
to send the Sui native token). Pure values can include primitive types
and vectors of primitive types, but not struct types.

The function to be invoked by the call is specified via an object
reference (which must refer to a package object), a name of a module in
that package, and a name of a function in that package. To execute a
call transaction, the Sui runtime will resolve the function, bind the
type, object, and value arguments to the function parameters, and use
the Move VM to execute the function.

Both call and publish transactions are subject to gas metering and gas
fees. The metering limit is expressed by a maximum gas budget. The
runtime will execute the transaction until the budget is reached, and
will abort with no effects (other than deducting fees and reporting the
abort code) if the budget is exhausted.

The fees are deducted from a *gas object* specified as an object
reference. This object must be a Sui native token (i.e., its type must
be ). Sui uses EIP1559[^1]-style fees: the protocol defines a base fee
(denominated in gas units per Sui token) that is algorithmically
adjusted at epoch boundaries, and the transaction sender can also
include an optional tip (denominated in Sui tokens). Under normal system
load, transactions will be processed promptly even with no tip. However,
if the system is congested, transactions with a larger tip will be
prioritized. The total fee deduced from the gas object is
$(\mathsf{GasUsed} * \mathsf{BaseFee}) + \mathsf{Tip}$.

## Transaction Effects

::: {#tab:effect}
  ----------------------------- --------------------------------------------------------------------------------------
  $\mathsf{Event} =$            $\StructType \times \Struct$
  $\mathsf{Create} =$           $\Object$
  $\mathsf{Update} =$           $\Object$
  $\mathsf{Wrap} =$             $\objectid \times \Version$
  $\mathsf{Delete} =$           $\objectid \times \Version$
  $\mathsf{ObjEffect} =$        $\mathsf{Create} \uplus \mathsf{Update} \uplus \mathsf{Wrap} \uplus \mathsf{Delete}$
  $\mathsf{AbortCode} =$        $\mathbb{N} \times \ModuleName$
  $\mathsf{SuccessEffects} =$   $\seq{\mathsf{ObjEffect}} \times \seq{\mathsf{Event}}$
  $\mathsf{AbortEffects} =$     $\mathsf{AbortCode}$
  $\mathsf{TxEffects} =$        $\mathsf{SuccessEffects} \uplus \mathsf{AbortEffects}$
  ----------------------------- --------------------------------------------------------------------------------------

  : Transaction Effects
:::

Transaction execution generates transaction effects which are different
in the case when execution of a transaction is successful ( in
Table [6](#tab:effect){reference-type="ref" reference="tab:effect"}) and
when it is not ( in Table [6](#tab:effect){reference-type="ref"
reference="tab:effect"}).

Upon successful transaction execution, transaction effects include
information about changes made to Sui's global object pool (including
both updates to existing objects and freshly created objects) and
*events* generated during transaction execution. Another effect of
successful transaction execution could be object removal (i.e.,
deletion) from the global pool and also wrapping (i.e., embedding) one
object into another, which has a similar effect to removal -- a wrapped
object disappears from the global pool and exists only as a part of the
object that wraps it. Since deleted and wrapped objects are no longer
accessible in the global pool, these effects are represented by the ID
and version of the object.

Events encode side effects of successful transaction execution beyond
updates to the global object pool. Structurally, an event consists of a
Move struct and its type. Events are intended to be consumed by actors
outside the blockchain, but cannot be read by Move programs.

Transactions in Move have an all-or-nothing semantics -- if execution of
a transaction aborts at some point (e.g., due to an unexpected failure),
even if some changes to objects had happened (or some events had been
generated) prior to this point, none of these effects persist in an
aborted transaction. Instead, an aborted transaction effect includes a
numeric abort code and the name of a module where the transaction abort
occurred. Gas fees are still charged for aborted transactions.

[^1]: <https://github.com/ethereum/EIPs/blob/master/EIPS/eip-1559.md>

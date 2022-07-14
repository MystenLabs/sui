This section provides a semi-formal specification for the validation and
execution of FastX transactions . We use the term \"validating\" to
describe the authority-side processing performed on a $\transaction$,
and \"executing\" to describe the authority-side processing performed on
a $\cert$. We first describe the steps common to processing any
transaction or certificate, then describe steps specific to the payload
(i.e., $\mathsf{Call}$ or $\Publish$) of the transaction/certificate.

  **Addresses and ID's**   
  ------------------------ -------------------------------------------------------------------------------------
  $\Authenticator =$       $\mathsf{PubKey} \uplus \objectid \uplus \ldots$
  $\AccountAddress =$      $Com(\Authenticator)$
  $\transactiondigest =$   $Com(\transaction)$
  $\objectid =$            $Com(\transactiondigest \times \mathbb{N})$
  $\ModuleID =$            $\objectid \times \ModuleName$
  **Types and Kinds**      
  $\GroundType =$          $\set{\texttt{address}, \texttt{bool}, \texttt{u64}, \texttt{bytes}}$
  $\StructType =$          $\ModuleID \times \StructName$
  $\StoreableType =$       $\GroundType \uplus \StructType$
  $\MutabilityQual =$      $\set{\texttt{mut}, \texttt{immut}}$
  $\ReferenceType =$       $\StoreableType \times \MutabilityQual$
  $\Type =$                $\mathsf{ReferenceType} \uplus \StoreableType$
  $\Kind =$                $\set{\texttt{key}, \texttt{store}, \texttt{copy}, \texttt{drop}}$
  **Values and State**     
  $\PrimValue=$            $\AccountAddress \uplus \mathsf{Bytes} \uplus \mathbb{B} \uplus \mathbb{N}$
  $\Struct =$              $\FieldName \pfun \StoreableValue$
  $\StoreableValue =$      $\Struct \uplus \PrimValue$
  $\Reference =$           $\Var \times \seq{\FieldName}$
  $\Value =$               $\StoreableValue \uplus \Reference$
  $\LocalEnv =$            $\Var \pfun \Value$
  $\CallStackFrame =$      $\ProcID \times \PC \times \LocalEnv$
  $\CallStack =$           $\seq{\CallStackFrame}$
  $\OperandStack =$        $\seq{\Value}$
  $\textsf{State} =$       $\CallStack \times \OperandStack$
  **Modules**              
  $\Module =$              $\ModuleID \times (\StructName \pfun \StructDecl)$
                           $\times (\ProcName \pfun \ProcDecl) \times \seq{\ModuleID}$
  $\StructDecl =$          $\seq{\Kind} \times(\FieldName \pfun \StoreableType)$
  $\ProcDecl$ =            $\seq{\seq{\Kind}} \times \seq{\Type} \times \seq{\Type} \times \seq{\Instruction}$
  $\Instruction=$          $\ldots$
  **Events**               
  $\Transfer =$            $\ObjID \times \Struct \times \StructType \times \AccountAddress$
  $\Freeze =$              $\ObjID \times \StructType \times \Struct$
  $\Event =$               $\Transfer \uplus \Freeze \uplus \ldots$

  **Objects**                  
  ---------------------------- ---------------------------------------------------------------------------------------------
  $\mathsf{MoveObjType} =$     $\StructType \times \MutabilityQual$
  $\mathsf{ObjType} =$         $\mathsf{MoveObjType} \uplus \{ \texttt{code} \}$
  $\mathsf{ObjData} =$         $\Struct \uplus \Module$
  $\Object =$                  $\mathsf{ObjData} \times \mathsf{ObjType} \times \AccountAddress \times \transactiondigest$
                               $\times \Version$
  **Transactions**             
  $\ObjRef=$                   $\objectid \times \transactiondigest \times Com(\Object)$
  $\textsf{GasFee} =$          $\ObjRef$
  $\textsf{CallTarget}=$       $\ObjRef \times \ProcName$
  $\textsf{Call} =$            $\textsf{CallTarget} \times \seq{\StoreableType} \times \seq{\ObjRef}$
                               $\times \seq{\PrimValue} \times \textsf{GasBudget}$
  $\textsf{Publish} =$         $\Module \times \seq{\ObjRef}$
  $\UserSig =$                 $\AccountAddress \times \Authenticator \times \Sig$
  $\mathsf{AuthorityName} =$   $\mathsf{PublicKey}$
  $\AuthoritySig =$            $\mathsf{AuthorityName} \times \Sig$
  $\mathsf{TxMsg} =$           $(\textsf{Call} \uplus \textsf{Publish}) \times \textsf{GasFee} \times \textsf{GasPrice}$
                               $\times \textsf{EpochID}$
  $\transaction =$             $\mathsf{TxMsg} \times \UserSig$
  $\textsf{SignedTx} =$        $\transaction \times \AuthoritySig$
  $\cert =$                    $\transaction \times \seq{\AuthoritySig}$
  **Stores**                   
  $\mathsf{LockMap} =$         $\ObjRef \pfun \transaction_{\bot}$
  $\mathsf{CertMap} =$         $\transactiondigest \pfun \transaction \times \cert$
  $\mathsf{ObjMap} =$          $\objectid \pfun \Object_{\bot}$
  $\mathsf{SyncMap} =$         $\objectid \times \transactiondigest \pfun \cert$

## Common Processing

#### Transaction Prologue

The following checks are performed on all $\transaction$'s. The
authority should authenticate the sender of the message and check that
its mutable input objects have not previously been used. In detail:

1.  $\transaction.\UserSig.\Sig$ is cryptographically valid signature on
    the message $\transaction.\mathsf{TxMsg}$ w.r.t
    $\transaction.\UserSig.\Authenticator$

2.  The transaction sender\
    $\transaction.\UserSig.\AccountAddress$ matches
    $Com(\UserSig.\Authenticator)$

3.  The $\EpochID$ matches the current epoch

4.  For all $\ObjRef$ inputs, $\objectid$ is unique (i.e., no duplicate
    inputs)

5.  $\ObjMap[\objectid]$ exists for all $\ObjRef$ inputs

6.  $\ObjRef.\transactiondigest = \ObjMap[\ObjRef.\objectid].\transactiondigest$
    for all $\ObjRef$'s (i.e., each object input correctly points to the
    transaction that created or last mutated it)

7.  $Com(\Object)$ matches $\ObjMap[\ObjRef.\objectid]$ for all
    $\ObjRef$ inputs, (i.e., each input commits to the correct value of
    its corresponding object in persistent storage)

8.  For all mutable $\ObjRef$ inputs, $\LockMap[\ObjRef]$ exists and
    either is not set, or is set to exactly the same transaction (i.e.,
    the authority is not waiting on confirmation of a different
    transaction accepting any of these objects as input)

#### Transaction Epilogue

If all of these checks pass, the authority should:

1.  Set $\LockMap[\ObjRef] = \transaction$ for all mutable $\ObjRef$
    inputs. TODO: explain how to determine input mutability--anything
    passed by value or as a mutable reference. Immutable objects, or
    immutable references to mutable objects don't count as mutable

2.  Sign the message to create an $\AuthoritySig$ and return a
    $\SignedTx$ to the user.

#### Certificate Prologue

The following checks should be performed prior to processing any
$\cert$:

1.  For each $\AuthoritySig$ in the certificate,
    $\cert.\AuthoritySig.\Sig$ is cryptographically valid signature on
    the message $\transaction$ w.r.t public key
    $\cert.\AuthoritySig.\mathsf{AuthorityName}$

2.  $\cert.\transaction.\EpochID$ matches the current epoch

3.  $\AuthoritySig.\mathsf{AuthorityName}$ is an authority in the
    current epoch

4.  $|\seq{\AuthoritySig}| \geq N - F$, where $N$ is the number of
    authorities in the current epoch and $F$ the number of failures
    tolerated (i.e., the certificate has signatures from a quorum of
    authorities. Note that $N > 3F$.)

5.  $\LockMap[\ObjRef]$ exists for each mutable $\ObjRef$ input to the
    transaction TODO: this is needed to prevent out of order execution.
    but we could consider relaxing this if we are comfortable with out
    of order exec.

#### Certificate Epilogue

The following actions should be performed at the conclusion of
processing any $\cert$:

1.  Update the value of $\ObjMap[\GasFee.\ObjID].\ObjData$ to the gas
    used during execution multiplied by $\GasPrice$ (i.e., pay for gas)

2.  Increment $\ObjMap[\GasFee.\ObjID].\Version$ (i.e., reflect that the
    gas object has been mutated via paying for gas)

3.  Update $\ObjMap[\GasFee.\ObjID].\transactiondigest$ to the digest of
    the current transaction (i.e., reflect that this transaction mutated
    the gas fee object)

4.  Delete entry $\LockMap[\ObjRef]$ for all mutable $\ObjRef$ inputs
    (whether it is set of not).

5.  Update $\CertMap$ with the current $\transaction$ and $\cert$

## Processing a $\mathsf{Call}$

#### $\mathsf{Call}$ Transaction Prologue

For a $\transaction$ containing a $\mathsf{Call}$ payload, the authority
should perform these additional validation checks as part of the
prologue:

1.  For the $\GasFee$, the value of
    $\ObjMap[\GasFee.\objectid].\ObjData$ object is greater than or
    equal to $\GasBudget * \GasPrice$ (i.e., the fee is sufficient)
    TODO: explain how to extract the value

#### $\mathsf{Call}$ Certificate Prologue

The input is a $\cert$ containing a $\mathsf{Call}$ payload. In addition
to performing the certificate prologue described above, the authority
should type-check the call:

1.  $\ObjMap[\CallTarget.\ObjID].\ObjType$ is `code`

2.  $\CallTarget.\ProcName$ is a public function declared by this module

3.  The arity of the function is
    $|\seq{\ObjRef}| + |\seq{\PrimValue}| + 1$. TODO: explain the
    +1--for a special transaction-local `TxContext` object constructed
    by the adapter using the $\transactiondigest$ and sender address

4.  For all $\ObjRef$'s, the type of the $i$th input\
    $\ObjMap[\ObjID_i].\ObjType.\StructType$ is equal to the
    reference-stripped type of the $i$th parameter in the type signature
    of the function (i.e., the transaction inputs are well-typed w.r.t
    the function). TODO: explain reference stripping

5.  For all mutable $\ObjRef$ inputs, the type of the $i$th input\
    $\ObjMap[\ObjID_i].\ObjType.\MutabilityQual$ is `mut` (i.e., the
    transaction does not attempt to mutate immutable objects)

6.  The function has no return types

7.  For all$\seq{\PrimValue}$ arguments, the $i$th argument can be
    deserialized using to the $i + |\seq{\ObjRef}|$th parameter type
    (i.e., each primitive value argument is well-typed w.r.t the
    function signature).

8.  For each $\StructType$ argument in the type arguments\
    $\seq{\StoreableType}$, the entry
    $\ObjMap[\StructType.\ModuleID.\ObjID]$ exists (i.e., the type
    arguments refer to valid types)

If these checks pass, the authority should run the code embedded in the
payload. In detail:

1.  Bind the type arguments in $\seq{\StoreableType}$ to the type
    parameters of the function

2.  Create arguments by concatenating $\Struct$'s extracted from the
    object inputs inputs to the $\PrimValue$ inputs. TODO: more details
    on how this works, particularly with reference inputs

3.  Bind these arguments to the parameters of the $\ProcDecl$ and
    execute the entrypoint using the Move VM

#### $\mathsf{Call}$ Certificate Epilogue

Execution terminates with the amount of gas used and a status code
indicating whether execution concluded successfully or *aborted*. If
execution succeeds, the outputs are (A) changes to the mutable input
$\Struct$'s that were passed by reference and (B) a log of events
emitted by execution . The events in (B) may include distinguished
$\Transfer$ and $\Freeze$ events with special interpretations, as well
as user-defined events.

In the case of a successful execution, the authority should:

1.  Update the $\ObjData$ of each object in (A) in $\ObjMap$. This
    reflects the modifications made to objects that were passed to the
    entrypoint via a mutable reference.

2.  For each object passed by value to the entrypoint: create a
    temporary map $\mathsf{Inputs}: \ObjID \pfun \Object$ reflecting the
    value of each object before the function was called. Delete each
    object that was passed by value to the entrypoint by writing
    $\ObjMap[\ObjRef.\ObjID] = \bot$ for each input $\ObjRef$ passed by
    value

3.  For each $\Transfer$ event in (B): if the $\ObjID$ *is not* in
    $\mathsf{Inputs}$, create a new $\Object$ with $\ObjData$ containing
    the $\Struct$ from the event, $\ObjType$ as the $\StructType$ from
    the event plus $\texttt{mut}$ as a $\MutabilityQual$,
    $\AccountAddress$ as the address from the event,
    $\transactiondigest$ as the digest of the current transaction, and
    $\Version$ 0. (i.e., transfer an object created by this transaction)

4.  For each $\Freeze$ event in (B): if the $\ObjID$ *is not* in
    $\mathsf{Inputs}$, create a new $\Object$ with $\ObjData$ containing
    the $\Struct$ from the event plus $\texttt{immut}$ as a
    $\MutabilityQual$,, $\ObjType$ as the $\StructType$ from the event,
    $\AccountAddress$ as $\UserSig.\AccountAddress$,
    $\transactiondigest$ as the digest of the current transaction, and
    $\Version$ 0. (i.e., freeze an object created by this transaction)

5.  For each $\Transfer$ event in (B): if the $\ObjID$ *is* in
    $\mathsf{Inputs}$, update $\mathsf{Inputs}[\ObjID].\ObjData$ to the
    $\Struct$ from the event, and
    $\mathsf{Inputs}[\ObjID].\AccountAddress$ to the $\AccountAddress$
    (i.e., transfer an object passed by value to this transaction).
    Update $\ObjMap[\ObjID]$ with $\mathsf{Inputs}[\ObjID]$ for each of
    these objects.

6.  For each $\Freeze$ event in (B): if the $\ObjID$ *is* in
    $\mathsf{Inputs}$, update $\mathsf{Inputs}[\ObjID].\ObjData$ to the
    $\Struct$ from the event, and
    $\mathsf{Inputs}[\ObjID].\ObjType.\MutabilityQual$ to
    $\texttt{immut}$. (i.e., freeze an object passed by value to this
    transaction). Update $\ObjMap[\ObjID]$ with
    $\mathsf{Inputs}[\ObjID]$ for each of these objects.

7.  For each $\ObjID$ in $\mathsf{Inputs}$ that did not appear in a
    $\Freeze$ or $\Transfer$ event, delete the object by updating
    $\ObjMap[\ObjID] = \bot$ (i.e., delete inputs passed by value that
    were neither transferred nor frozen) TODO: should we leave the
    $\Object$ but set its $\ObjData$ to $\bot$?

8.  Increment the $\Version$ of each object in (A) and the
    transferred/frozen objects that were not freshly created

9.  Update the $\transactiondigest$ of each object in (A) and the
    transferred/frozen objects to the digest of the current transaction

10. For each $\Transfer$ event in (B) and each mutable object in (A):
    update $\LockMap[\ObjID \times \transactiondigest] = \transaction$,
    where $\transaction$ is the current transaction and
    $\transactiondigest$ is the digest of the current transaction (i.e.,
    create a lock for each new mutable $\ObjRef$).

11. TODO: describe processing of use-defined events

In the case of either successful execution, aborted execution, or failed
typechecking of the function, the authority should perform the
transaction epilogue described above. TODO: say what gas used should be
if typechecking fails

## Processing a $\Publish$

#### $\Publish$ Transaction Prologue

For a $\transaction$ containing a $\Publish$ payload, the authority
should perform these validation checks in addition to the transaction
prologue described above:

1.  For the $\GasFee$, the value of $\ObjMap[\objectid].\ObjData$ object
    is greater than or equal to the $\GasPrice$ multiplied by the size
    of the module in bytes (i.e., the fee is sufficient) TODO: may want
    a flat fee or additional multiplier

2.  $\ObjMap[\ObjRef.\ObjID].\ObjType$ is `code` for each $\ObjRef$
    included in the payload (i.e., each input is a module)

3.  The list $\seq{\ObjRef.\ObjID].\ObjData.\ModuleID}$ for each
    $\ObjRef$ included in the payload is equal to
    $\Module.\seq{\ModuleID}$ (i.e., the declared dependencies of the
    module exactly match the $\ObjRef$'s passed as input

#### $\Publish$ Certificate Prologue

The input is a $\cert$ containing a $\Publish$ payload. The authority
should first verify the module:

1.  Run the Move bytecode verifier on the $\Module$ in the payload

2.  Run the Move linker on the $\Module$ and its dependencies (i.e.,
    ensure that the handles in the current module match their
    declarations in its dependencies)

3.  Run the FastX-specific bytecode verifier passes on the $\Module$ in
    the payload

4.  Check that the $\ObjID$ in the $\Module$ is 00\...0

#### $\Publish$ Certificate Epilogue

If verification passes, the authority should publish the module as an
object:

1.  Create a fresh $\ObjID$ for the module from the $\transactiondigest$
    of the current transaction and an $\mathsf{OutputIndex}$ of 0

2.  Overwrite the existing $\ObjID$ of the $\Module$ with this fresh ID.

3.  Create an $\Object$ containing this modified $\Module$, type `code`,
    the $\AccountAddress$ of the transaction sender,
    $\transactiondigest$ of the current transaction, and $\Version$ 0.
    Insert this object into $\ObjMap[\ObjID]$.

In the case of either successful verification or failed verification,
the authority should perform the transaction epilogue described above.

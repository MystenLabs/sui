In this section we describe from a systems' perspective, including the
mechanisms to ensure safety and liveness across authorities despite
Byzantine failures. We also explain the operation of clients, including
light clients that need some assurance about the system state without
validating its full state.

**Brief background.** At a systems level is an evolution of the
FastPay [@BaudetDS20] low-latency settlement system, extended to operate
on arbitrary objects through user-defined smart contracts, and with a
permissionless delegated proof of stake committee
composition [@BanoSAAMMD19]. Basic asset management by object owners is
based on a variant of Byzantine consistent
broadcast [@cachin2011introduction] that has lower latency and is easier
to scale across many machines as compared to traditional implementations
of Byzantine
consensus [@GuerraouiKMPS19; @DBLP:journals/corr/abs-1812-10844; @DBLP:conf/dsn/CollinsGKKMPPST20].
When full agreement is required we use a high-throughput DAG-based
consensus, e.g. [@narwhal] to manage locks, while execution on different
shared objects is parallelized.

**Protocol outline.** Figure [1](#fig:outline){reference-type="ref"
reference="fig:outline"} illustrates the high-level interactions between
a client and authorities to commit a transaction. We describe them here
briefly:

-   A user with a private signing key creates and signs a *user
    transaction* to mutate objects they own, or shared objects, within .
    Subsequently, user signature keys are not needed, and the remaining
    of the process may be performed by the user client, or a gateway on
    behalf of the user (denoted as *keyless operation* in the diagram).

-   The user transaction is sent to the authorities, that each check it
    for validity, and upon success sign it and return the *signed
    transaction* to the client. The client collects the responses from a
    quorum of authorities to form a *transaction certificate*.

-   The transaction certificate is then sent back to all authorities,
    and if the transaction involves shared objects it is also sent to a
    *Byzantine agreement protocol* operated by the authorities.
    Authorities check the certificate, and in case shared objects are
    involved also wait for the agreement protocol to sequence it in
    relation to other shared object transactions, and then execute the
    transaction and summarize its effects into a *signed effects*
    response.

-   Once a quorum of authorities has executed the certificate its
    effects are final (denoted as *finality* in the diagram). Clients
    can collect a quorum of authority responses and create an *effects
    certificate* and use it as a proof of the finality of the
    transactions effects.

This section describes each of these operations in detail, as well as
operations to reconfigure and manage state across authorities.

![Outline of interactions to commit a
transaction.](figs/diagam_sui.pdf){#fig:outline width="\\columnwidth"}

## System Model

operates in a sequence of epochs denoted by $e \in \{0, \ldots\}$. Each
epoch is managed by a committee $C_e = (V_e, S_e(\cdot))$, where $V_e$
is a set of authorities with known public verification keys and network
end-points. The function $S_e(v)$ maps each authority $v \in V_e$ to a
number of units of delegated stake. We assume that $C_e$ for each epoch
is signed by a quorum (see below) of authority stake at epoch $e-1$.
(Sect. [0.7](#sec:reconfig){reference-type="ref"
reference="sec:reconfig"} discusses the formation and management of
committees). Within an epoch, some authorities are correct (they follow
the protocol faithfully and are live), while others are Byzantine (they
deviate arbitrarily from the protocol). The security assumption is that
the set of honest authorities $H_e \subseteq V_e$ is assigned a *quorum*
of stake within the epoch,
i.e. $\sum_{h \in H_e} S_e(h) > 2/3 \sum_{v \in V_e} S_e(v)$ (and refer
to any set of authorities with over two-thirds stake as a quorum).

There exists at least one live and correct party that acts as a *relay*
for each certificate (see Sect. [0.3](#sec:base){reference-type="ref"
reference="sec:base"}) between honest authorities. This ensures
liveness, and provides an eventual delivery property to the Byzantine
broadcast (see totality of reliable broadcast
in [@cachin2011introduction]). Each authority operates such a relay,
either individually or through a collective dissemination protocol.
External entities, including light clients, replicas and services may
also take on this role. The distinction between the passive authority
core, and an internal or external active relay component that is less
reliable or trusted, ensures a clear demarcation and minimization of the
Trusted Computing Base [@saltzer1975protection] on which 's safety and
liveness relies.

## Authority & Replica Data Structures

authorities rely on a number of data structures to represent state. We
define these structures based on the operations they support. They all
have a deterministic byte representation.

An *Object* ($\object$) stores user smart contracts and data within .
They are the system-level encoding of the Move objects introduced in
Sect. [\[sec:move\]](#sec:move){reference-type="ref"
reference="sec:move"}. They support the following set of operations:

-   $\oref{\object}$ returns the *reference* () of the object, namely a
    triplet (, , ).  is practically unique for all new objects created,
    and  is an increasing positive integer representing the object
    version as it is being mutated.

-   $\oauth{\object}$ returns the authenticator  of the owner of the
    object. In the simplest case,  is an address, representing a public
    key that may use this object. More complex authenticators are also
    available (see Sect. [0.4](#sec:owners){reference-type="ref"
    reference="sec:owners"}).

-   $\oro{\object}$ returns true if the object is read-only. Read-only
    objects may never be mutated, wrapped or deleted. They may also be
    used by anyone, not just their owners.

-   $\parent{\object}$ returns the transaction digest () that last
    mutated or created the object.

-   $\contents{\object}$ returns the object type $\type$ and data
    $\data$ that can be used to check the validity of transactions and
    carry the application-specific information of the object.

The object reference () is used to index objects. It is also used to
authenticate objects since  is a commitment to their full contents.

A *transaction* ($\transaction$) is a structure representing a state
transition for one or more objects. They support the following set of
operations:

-   $\txdigest{\transaction}$ returns the $\transactiondigest$, which is
    a binding cryptographic commitment to the transaction.

-   $\txepoch{\transaction}$ returns the $\EpochID$ during which this
    transaction may be executed.

-   $\txinputs{\transaction}$ returns a sequence of object
    $[ \objectref ]$ the transaction needs to execute.

-   $\txecon{\transaction}$ returns a reference to an  to be used to pay
    for gas, as well as the maximum gas limit, and a conversion rate
    between a unit of gas and the unit of value in the gas payment
    object.

-   $\txvalid{\transaction, [\object]}$ returns true if the transaction
    is valid, given the requested input objects provided. Validity is
    discussed in Sect. [0.4](#sec:owners){reference-type="ref"
    reference="sec:owners"}, and relates to the transactions being
    authorized to act on the input objects, as well as sufficient gas
    being available to cover the costs of its execution.

-   $\txexec{\transaction, [\object]}$ executes the transaction and
    returns a structure $\effects$ representing its effects. A valid
    transaction execution is infallible, and its output is
    deterministic.

A transaction is indexed by its $\transactiondigest$, which may also be
used to authenticate its full contents. All valid transactions (except
the special hard-coded genesis transaction) have at least one owned
input, namely the objects used to pay for gas.

A *transaction effects* () structure summarizes the outcome of a
transaction execution. It supports the following operations:

-   $\txdigest{\effects}$ is a commitment  to the  structure, that may
    be used to index or authenticate it.

-   $\efftransaction{\effects}$ returns the $\transactiondigest$ of the
    executed transaction yielding the effects.

-   $\txdeps{\effects}$ returns a sequence of dependencies
    $[ \transactiondigest ]$ that should be executed before the
    transaction with these effects may execute.

-   $\contents{\effects}$ returns a summary of the execution. $\status$
    reports the outcome of the smart contract execution. The lists
    $\created$, $\mutated$, $\wrapped$, $\unwrapped$ and $\deleted$,
    list the object references that underwent the respective operations.
    And $\events$ lists the events emitted by the execution.

A *transaction certificate* $\cert$ on a transaction contains the
transaction itself as well as the identifiers and signatures from a
quorum of authorities. Note that a certificate may not be unique, in
that the same logical certificate may be represented by a different set
of authorities forming a quorum. Additionally, a certificate might not
strictly be signed by exactly a 2/3 quorum, but possibly more if more
authorities are responsive. However, two different valid certificates on
the same transaction should be treated as representing semantically the
same certificate. A *partial certificate* () contains the same
information, but signatures from a set of authorities representing stake
lower than the required quorum, usually a single authority. The
identifiers of signers are included in the certificate (i.e.,
accountable signatures [@boneh2018compact]) to identify authorities
ready to process the certificate, or that may be used to download past
information required to process the certificate (see
Sect. [0.8](#sec:sync){reference-type="ref" reference="sec:sync"}).

Similarly, an *effects certificate* $\ecert$ on an effects structure
contains the effects structure itself, and signatures from
authorities[^1] that represent a quorum for the epoch in which the
transaction is valid. The same caveats, about non-uniqueness and
identity apply as for transaction certificates. A partial effects
certificate, usually containing a single authority signature and the
effects structure is denoted as $\esign$.

**Persistent Stores.** Each authority and replica maintains a set of
persistent stores. The stores implement persistent map semantics and can
be represented as a set of key-value pairs (denoted
$map[key] \rightarrow value$), such that only one pair has a given key.
Before a pair is inserted a $\mathsf{contains}(key)$ call returns false,
and $\mathsf{get}(key)$ returns an error. After a pair is inserted
$\mathsf{contains}(key)$ calls returns true, and $\mathsf{get}(key)$
return the value. An authority maintains the following persistent
stores:

-   The *order lock map*
    $\txdb_v[\objectref] \rightarrow \txsign \textsf{Option}$ records
    the first valid transaction $\transaction$ seen and signed by the
    authority for an owned object version $\objectref$, or None if the
    object version exists but no valid transaction using as an input it
    has been seen. It may also record the first certificate seen with
    this object as an input. This table, and its update rules,
    represents the state of the distributed locks on objects across
    authorities, and ensures safety under concurrent processing of
    transactions.

-   The *certificate map*
    $\certdb_v[\transactiondigest] \rightarrow (\cert, \esign)$ records
    all full certificates $\cert$, which also includes $\transaction$,
    processed by the authority within their validity epoch, along with
    their signed effects $\esign$. They are indexed by transaction
    digest $\transactiondigest$

-   The *object map* $\objdb_v[\objectref] \rightarrow \object$ records
    all objects $\object$ created by transactions included in
    certificates within $\certdb_v$ indexed by $\objectref$. This store
    can be completely derived by re-executing all certificates in
    $\certdb_v$. A secondary index is maintained that maps $\objectid$
    to the latest object with this ID. This is the only information
    necessary to process new transactions, and older versions are only
    maintained to facilitate reads and audit.

-   The *synchronization map*
    $\syncdb_v[\objectref] \rightarrow \transactiondigest$ indexes all
    certificates within $\certdb_v$ by the objects they create, mutate
    or delete as tuples $\objectref$. This structure can be fully
    re-created by processing all certificates in $\certdb_v$, and is
    used to help client synchronize transactions affecting objects they
    care about.

Authorities maintain all four structures, and also provide access to
local checkpoints of their certificate map to allow other authorities
and replicas to download their full set of processed certificates. A
replica does not process transactions but only certificates, and
re-executes them to update the other tables as authorities do. It also
maintains an order lock map to audit non-equivocation.

An authority may be designed as a full replica maintaining all four
stores (and checkpoints) to facilitate reads and synchronization,
combined with a minimal authority core that only maintains object locks
and objects for the latest version of objects used to process new
transactions and certificates. This minimizes the Trusted Computing Base
relied upon for safety.

Only the order lock map requires strong key self-consistency, namely a
read on a key should always return whether a value or None is present
for a key that exists, and such a check should be atomic with an update
that sets a lock to a non-None value. This is a weaker property than
strong consistency across keys, and allows for efficient sharding of the
store for scaling. The other stores may be eventually consistent without
affecting safety.

## Authority Base Operation {#sec:base}

**Process Transaction.** Upon receiving a transaction $\transaction$ an
authority performs a number of checks:

1.  It ensures $\txepoch{\transaction}$ is the current epoch.

2.  It ensures all object references $\txinputs{\transaction}$ and the
    gas object reference in $\txecon{\transaction}$ exist within
    $\objdb_v$ and loads them into $[\object]$. For owned objects the
    exact reference should be available; for read-only or shared objects
    the object ID should exist.

3.  Ensures sufficient gas can be made available in the gas object to
    cover the cost of executing the transaction.

4.  It checks $\txvalid{\transaction, [\object]}$ is true. This step
    ensures the authentication information in the transaction allows
    access to the owned objects.

5.  It checks that $\txdb_v[\objectref]$ for all owned
    $\txinputs{\transaction}$ objects exist, and it is either None or
    set to *the same* $\transaction$, and atomically sets it to
    $\txsign$. (We call these the 'locks checks').

If any of the checks fail processing ends, and an error is returned.
However, it is safe for a partial update of $\txdb_v$ to persist
(although our current implementation does not do partial updates, but
atomic updates of all locks).

If all checks are successful then the authority returns a signature on
the transaction, ie. a partial certificate . Processing an order is
idempotent upon success, and returns a partial certificate (), or a full
certificate () if one is available.

Any party may collate a transaction and signatures () for a set of
authorities forming a quorum for epoch $e$, to form a transaction
certificate $\cert$.

**Process Certificate.** Upon receiving a certificate an authority
checks all validity conditions for the transaction, except those
relating to locks (the so-called 'locks checks'). Instead it performs
the following checks: for each *owned* input object in
$\txinputs{\transaction}$ it checks that the lock exists, and that it is
either None, set to *any* , or set to a certificate for the same
transaction as the current certificate. If this modified locks check
fails, the authority has detected an unrecoverable Byzantine failure,
halts normal operations, and starts a disaster recovery process. For
*shared objects* (see Sect. [0.4](#sec:owners){reference-type="ref"
reference="sec:owners"}) authorities check that the locks have been set
through the certificate being sequenced in a consensus, to determine the
version of the share object to use. If so, the transaction may be
executed; otherwise it needs to wait for such sequencing first.

If the check succeeds, the authority adds the certificate to its
certificate map, along with the effects resulting from its execution,
ie. $\certdb_v[\transactiondigest] \rightarrow (\cert, \esign)$; it
updates the locks map to record the certificate
$\txdb_v[\objectref] \rightarrow \cert$ for all owned input objects that
have locks not set to a certificate. As soon as all objects in
$\txinput(\transaction)$ is inserted in $\objdb_v$, then all effects in
$\esign$ are also materialized by adding their $\objectref$ and contents
to $\objdb_v$. Finally for all created or mutated in $\esign$ the
synchronization map is updated to map them to $\transaction$.

**Remarks.** The logic for handling transactions and certificates leads
to a number of important properties:

-   **Causality & parallelism.** The processing conditions for both
    transactions and certificates ensure causal execution: an authority
    only 'votes' by signing a transaction if it has processed all
    certificates creating the objects the transaction depends upon, both
    owned, shared and read-only. Similarly, an authority only processes
    a certificate if all input objects upon which it depends exist in
    its local objects map. This imposes a causal execution order, but
    also enables transactions not causally dependent on each other to be
    executed in parallel on different cores or machines.

-   **Sign once, and safety.** All owned input objects locks in
    $\txdb_v[\cdot]$ are set to the first transaction $\transaction$
    that passes the checks using them, and then the first certificate
    that uses the object as an input. We call this *locking the object
    to this transaction*, and there is no unlocking within an epoch. As
    a result an authority only signs a single transaction per lock,
    which is an essential component of consistent
    broadcast [@cachin2011introduction], and thus the safety of .

-   **Disaster recovery.** An authority detecting two contradictory
    certificates for the same lock, has proof of irrecoverable Byzantine
    behaviour -- namely proof that the quorum honest authority
    assumption does not hold. The two contradictory certificates are a
    fraud proof [@Al-BassamSBK21], that may be shared with all
    authorities and replicas to trigger disaster recovery processes.
    Authorities may also get other forms of proof of unrecoverable
    byzantine behaviour such as \>1/3 signatures on effects () that
    represent an incorrect execution of a certificate. Or a certificate
    with input objects that do not represent the correct outputs of
    previously processed certificates. These also can be packaged as a
    fraud proof and shared with all authorities and replicas. Note these
    are distinct from proofs that a tolerable minority of authorities
    ($\leq1/3$ by stake) or object owners (any number) is byzantine or
    equivocating, which can be tolerated without any service
    interruption.

-   **Finality.** Authorities return a certificate () and the signed
    effects () for any read requests for an index in $\txdb_v$,
    $\certdb_v$ and $\objdb_v$, $\syncdb_v$. A transaction is considered
    final if over a quorum of authorities reports $\transaction$ as
    included in their $\certdb_v$ store. This means that an effects
    certificate () is a transferable proof of finality. However, a
    certificate using an object is also proof that all dependent
    certificates in its causal path are also final. Providing a
    certificate to any party, that may then submit it to a super
    majority of authorities for processing also ensures finality for the
    effects of the certificate. Note that finality is later than
    fastpay [@BaudetDS20] to ensure safety under re-configuration.
    However, an authority can apply the effect of a transaction upon
    seeing a certificate rather than waiting for a commit.

## Owners, Authorization, and Shared Objects {#sec:owners}

Transaction validity (see Sect. [0.3](#sec:base){reference-type="ref"
reference="sec:base"}) ensures a transaction is authorized to include
all specified input objects in a transaction. This check depends on the
nature of the object, as well as the owner field.

*Read-only objects* cannot be mutated or deleted, and can be used in
transactions concurrently and by all users. Move modules for example are
read-only. Such objects do have an owner that might be used as part of
the smart contract, but that does not affect authorization to use them.
They can be included in any transaction.

*Owned objects* have an owner field. The owner can be set to an address
representing a public key. In that case, a transaction is authorized to
use the object, and mutate it, if it is signed by that address. A
transaction is signed by a single address, and therefore can use one or
more objects owned by that address. However, a single transaction cannot
use objects owned by more than one address. The owner of an object,
called a child object, can be set to the $\objectid$ of another object,
called the parent object. In that case the child object may only be used
if the parent object is included in the transaction, and the transaction
is authorized to use the object. This facility may be used by contracts
to construct efficient collections and other complex data structures.

*Shared objects* are mutable, but do not have a specific owner. They can
instead be included in transactions by different parties, and do not
require any authorization. Instead they perform their own authorization
logic. Such objects, by virtue of having to support multiple writers
while ensuring safety and liveness, do require a full agreement protocol
to be used safely. Therefore they require additional logic before
execution. Authorities process transactions as specified in
Sect. [0.3](#sec:base){reference-type="ref" reference="sec:base"} for
owned objects and read-only objects to manage their locks. However,
authorities do not rely on consistent broadcast to manage the locks of
shared objects. Instead, the creators of transactions that involve
shared objects insert the certificate on the transaction into a
high-throughput consensus system, e.g. [@narwhal]. All authorities
observe a consistent sequence of such certificates, and assign the
version of shared objects used by each transaction according to this
sequence. Then execution can proceed and is guaranteed to be consistent
across all authorities. Authorities include the version of shared
objects used in a transaction execution within the $\effects$
certificate.

The above rules ensure that execution for transactions involving
read-only and owned objects requires only consistent broadcast and a
single certificate to proceed; and Byzantine agreement is only required
for transactions involving shared objects. Smart contract authors can
therefore design their types and their operations to optimize transfers
and other operations on objects of a single user to have lower latency,
while enjoying the flexibility of using shared objects to implement
logic that needs to be accessed by multiple users.

## Clients

**Full Clients & Replicas.** Replicas, also sometimes called *full
clients*, do not validate new transactions, but maintain a consistent
copy of the valid state of the system for the purposes of audit, as well
as to construct transactions or operate services incl. read
infrastructures for light client queries.

**Light Clients.** Both object references and transactions contain
information that allows the authentication of the full causal chain of
transactions that leading up to their creation or execution.
Specifically, an object reference () contains an that is an
authenticator for the full state of the object, including the facility
to get $\parent{\object}$, namely the that created the object.
Similarly, a authenticates a transaction, including the facility to
extract through $\txinputs{\transaction}$ the object references of the
input objects. Therefore the set of objects and certificates form a
bipartite graph that is self-authenticating. Furthermore, effects
structures are also signed, and may be collated into effects
certificates that directly certify the results of transaction
executions.

These facilities may be used to support *light clients* that can perform
high-integrity reads into the state of , without maintaining a full
replica node. Specifically an authority or full node may provide a
succinct bundle of evidence, comprising a certificate $\cert$ on a
transaction $\transaction$ and the input objects $[\Object]$
corresponding to $\txinputs{\transaction}$ to convince a light client
that a transition can take place within . A light client may then submit
this certificate, or check whether it has been seen by a quorum or
sample of authorities to ensure finality. Or it may craft a transaction
using the objects resulting from the execution, and observe whether it
is successful.

More directly, a service may provide an effects certificate to a client
to convince them of the existence and finality of a transition within ,
with no further action or interaction within the system. If a checkpoint
of finalized certificates is available, at an epoch boundary or
otherwise, a bundle of evidence including the input objects and
certificate, alongside a proof of inclusion of the certificate in the
checkpoint is also a proof of finality.

Authorities may use a periodic checkpointing mechanism to create
collective checkpoints of finalized transactions, as well as the state
of over time. A certificate with a quorum of stake over a checkpoint can
be used by light clients to efficiently validate the recent state of
objects and emitted events. A check pointing mechanism is necessary for
committee reconfiguration between epochs. More frequent checkpoints are
useful to light clients, and may also be used by authorities to compress
their internal data structures as well as synchronize their state with
other authorities more efficiently.

## Bridges

Native support for light clients and shared objects managed by Byzantine
agreement allows to support two-way bridges to other
blockchains [@DBLP:journals/iacr/McCorryBYS21]. The trust assumption of
such bridges reflect the trust assumptions of and the other blockchain,
and do not have to rely on trusted oracles or hardware if the other
blockchain also supports light clients [@ChatzigiannisBC21a]. Sui will
provide the permission-less exhaustive access to state data required to
support trust-minimized bridge architectures.

Bridges are used to import an asset issued on another blockchain, to
represent it and use it as a wrapped asset within the system.
Eventually, the wrapped asset can be unlocked and transferred back to a
user on the native blockchain. Bridges can also allow assets issued on
to be locked, and used as wrapped assets on other blockchains.
Eventually, the wrapped object on the other system can be destroyed, and
the object on updated to reflect any changes of state or ownership, and
unlocked.

The semantics of bridged assets are of some importance to ensure wrapped
assets are useful. Fungible assets bridged across blockchains can
provide a richer wrapped representation that allows them to be divisible
and transferable when wrapped. Non-fungible assets are not divisible,
but only transferable. They may also support other operations that
mutates their state in a controlled manner when wrapped, which may
necessitate custom smart contract logic to be executed when they are
bridged back and unwrapped. is flexible and allows smart contract
authors to define such experiences, since bridges are just smart
contracts implemented in Move rather than native concepts -- and
therefore can be extended using the composability and safety guarantees
Move provides.

## Committee Reconfiguration {#sec:reconfig}

Reconfiguration occurs between epochs when a committee $C_e$ is replaced
by a committee $C_{e'}$, where $e' = e + 1$. Reconfiguration safety
ensures that if a transaction $\transaction$ was committed at $e$ or
before, no conflicting transaction can be committed after $e$. Liveness
ensures that if $\transaction$ was committed at or before $e$, then it
must also be committed after $e$.

We leverage the smart contract system to perform a lot of the work
necessary for reconfiguration. Within a system smart contract allows
users to lock and delegate stake to candidate authorities. During an
epoch, owners of coins are free to delegate by locking tokens,
undelegate by unlocking tokens or change their delegation to one or more
authorities.

Once a quorum of stake for epoch $e$ vote to end the epoch, authorities
exchange information to commit to a checkpoint, determine the next
committee, and change the epoch. First, authorities run a check pointing
protocol, with the help of an agreement protocol [@narwhal], to agree on
a certified checkpoint for the end of epoch $e$. The checkpoint contains
the union of all transactions, and potentially resulting objects, that
have been processed by a quorum of authorities. As a result if a
transaction has been processed by a quorum of authorities, then at least
one honest authorities that processed it will have its processed
transactions included in the end-of-epoch checkpoint, ensuring the
transaction and its effects are durable across epochs. Furthermore, such
a certified checkpoint guarantees that all transactions are available to
honest authorities of epoch $e$.

The stake delegation at the end-of-epoch checkpoint is then used to
determine the new set of authorities for epoch $e+1$. Both a quorum of
the old authorities stake and a quorum of the new authority stake signs
the new committee $C_{e'}$, and checkpoint at which the new epoch
commences. Once both set of signatures are available the new set of
authorities start processing transactions for the new epoch, and old
authorities may delete their epoch signing keys.

**Recovery.** It is possible due to client error or client equivocation
for an owned object to become 'locked' within an epoch, preventing any
transaction concerning it from being certified (or finalized). For
example, a client signing two different transactions using the same
owned object version, with half of authorities signing each, would be
unable to form a certificate requiring a quorum of signatures on any of
the two certificates. Recovery ensures that once epochs change such
objects are again in a state that allows them to be used in
transactions. Since, no certificate can be formed, the original object
is available at the start of the next epoch to be operated on. Since
transactions contain an epoch number, the old equivocating transactions
will not lock the object again, giving its owner a chance to use it.

**Rewards & cryptoeconomics.** has a native token , with a fixed supply.
is used to pay for gas, and is also be used as delegated stake on
authorities within an epoch. The voting power of authorities within this
epoch is a function of this delegated stake. At the end of the epoch
fees collected through all transactions processed are distributed to
authorities according to their contribution to the operation of , and in
turn they share some of the fees as rewards to addresses that delegated
stake to them. We postpone a full description of the token economics of
to a dedicated paper.

## Authority & Replica Updating {#sec:sync}

**Client-driven.** Due to client failures or non-byzantine authority
failures, some authorities may not have processed all certificates. As a
result causally related transactions depending on missing objects
generated by these certificates would be rejected. However, a client can
always update an honest authority to the point where it is able to
process a correct transaction. It may do this using its own store of
past certificates, or using one or more other honest authorities as a
source for past certificates.

Given a certificate $c$ and a $Ct_v$ store that includes $c$ and its
causal history, a client can update an honest authority $v'$ to the
point where $c$ would also be applied. This involves finding the
smallest set of certificates not in $v'$ such that when applied the
Objects in $v'$ include all inputs of $c$. Updating a lagging authority
$B$ using a store $Ct_v$ including the certificate $\cert$ involves:

-   The client maintains a list of certificates to sync, initially set
    to contain just $\cert$.

-   The client considers the last $\cert$ requiring sync. It extracts
    the $\transaction$ within the $\cert$ and derives all its input
    objects (using $\txinput(\transaction)$).

-   For each input object it checks whether the $\transaction$ that
    generated or mutated last (using the $\syncdb_v$ index on $Ct_v$)
    has a certificate within $B$, otherwise its certificate is read from
    $Ct_v$ and added to the list of certificates to sync.

-   If no more certificates can be added to the list (because no more
    inputs are missing from $B$) the certificate list is sorted in a
    causal order and submitted to $B$.

The algorithm above also applies to updating an object to a specific
version to enable a new transaction. In this case the certificate for
the $\transaction$ that generated the object version, found in
$\syncdb_v[\objectref]$, is submitted to the lagging authority. Once it
is executed on $B$ the object at the correct version will become
available to use.

A client performing this operation is called a *relayer*. There can be
multiple relayers operating independently and concurrently. They are
untrusted in terms of integrity, and their operation is keyless. Besides
clients, authorities can run the relayer logic to update each other, and
replicas operating services can also act as relayers to update lagging
authorities.

**Bulk.** Authorities provide facilities for a follower to receive
updates when they process a certificate. This allows replicas to
maintain an up-to-date view of an authorty's state. Furthermore,
authorities may use a push-pull gossip network to update each other of
the latest processed transaction in the short term and to reduce the
need for relayers to perform this function. In the longer term lagging
authorities may use periodic state commitments, at epoch boundaries or
more frequently, to ensure they have processed a complete set of
certificates up to certain check points.

[^1]: Note that if the signature algorithm permits it, authority
    signatures can be compressed, but always using accountable signature
    aggregation, because tracking who signed is important for gas profit
    distribution and other network health measurements.

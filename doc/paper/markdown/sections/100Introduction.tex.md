is a decentralized permissionless smart contract platform biased towards
low-latency management of assets. It uses the Move programming language
to define assets as objects that may be owned by an address. Move
programs define operations on these typed objects including custom rules
for their creation, the transfer of these assets to new owners, and
operations that mutate assets.

is maintained by a permissionless set of authorities that play a role
similar to validators or miners in other blockchain systems. It uses a
Byzantine consistent broadcast protocol between authorities to ensure
the safety of common operations on assets, ensuring lower latency and
better scalability as compared to Byzantine agreement. It only relies on
Byzantine agreement for the safety of shared objects. As well as
governance operations and check-pointing, performed off the critical
latency path. Execution of smart contracts is also naturally
parallelized when possible. supports light clients that can authenticate
reads as well as full clients that may audit all transitions for
integrity. These facilities allow for trust-minimized bridges to other
blockchains.

A native asset is used to pay for gas for all operations. It is also
used by its owners to delegate stake to authorities to operate within
epochs, and periodically, authorities are reconfigured according to the
stake delegated to them. Used gas is distributed to authorities and
their delegates according to their stake and their contribution to the
operation of .

This whitepaper is organized in two parts, with
Sect. [\[sec:move\]](#sec:move){reference-type="ref"
reference="sec:move"} describing the programming model using the Move
language, and Sect. [\[sec:system\]](#sec:system){reference-type="ref"
reference="sec:system"} describing the operations of the permissionless
decentralized system that ensures safety, liveness and performance for .

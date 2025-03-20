These packages are part of the tree shaking algorithm tests
- if a dependency is added to the manifest, but it is not referenced in the code it needs to be removed

Edge cases
- if a package was published before the tree shaking change, it will have in the linkage table all the 
packages that it depends on, even if code from that packages are not referenced in the code
- in this case, we need to ensure that all trans deps of immediate used packages are considered accordingly.


Tests projects are established as following

- A is just a normal package, no deps.
    - linkage table should be empty
- A_v1 is a package upgrade of A.
    - linkage table should be empty
- A_v2 is a package upgrade of A (now at version 2).
    - linkage table should be empty
- B_A is a normal package that depends on A, and source code references A.
    - linkage table should contain package A's ID
- B_A1 is a package that depends on A, but source code does not reference any code from A.
     - linkage table should be empty
- C_B_A is a normal package that depends on B_A, and source code references B.
     - linkage table should contain package B's ID and package A's ID
- C_B is a package that depends on B, but source code does not reference any code from B.
     - linkage table should be empty
- D_A_v1 is a normal package that depends on A v1, and source code references A.
     - linkage table should contain package A's ID (and the related upgrade info)
- D_A is a package that depends on A v1, but source code does not reference any code from A.
     - linkage table should be empty
- E is a package that depends on A_v1 and on B_A
     - linkage table should contain package A's ID (and the related upgrade info) and package B's ID
- E_A_v1
    - linkage table should be empty
- F is a package that depends on A which is set to be a bytecode dep
    - linkage table should be empty
- G just a normal package that is not published
- H is just a package that depends on G
    - linkage table should be empty
- I depends_on_D A but_no code references A, and depends on A_v2
    - linkage table should be empty
- K is a normal package
- K_v2 is a package upgrade of K
- L is a package that has a code dependency on K
    - linkage table should contain package K's ID
- M has a code dependency on L_depends_on K package, and a dependency on K_v2 but no code references K_v2
    - linkage table should contain package K's ID and L's ID

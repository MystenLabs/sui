These packages are part of the tree shaking algorithm tests
- if a dependency is added to the manifest, but it is not referenced in the code it needs to be removed

Edge cases
- if a package was published before the tree shaking change, it will have in the linkage table all the 
packages that it depends on, even if code from that packages are not referenced in the code
- in this case, we need to fetch all the linkage table of all packages that are dependencies in the package.



Tests projects are established as following

- A is just a normal package, no deps.
    - linkage table should be empty
- A_v1 is a package upgrade of A.
    - linkage table should be empty
- B_depends_on_A is a normal package that depends on A, and source code references A.
    - linkage table should contain package A's ID
- B_depends_on_A_but_no_code_references_A is a package that depends on A, but source code does not reference any code from A.
     - linkage table should be empty
- C_depends_on_B is a normal package that depends on B, and source code references B.
     - linkage table should contain package B's ID and package A's ID
- C_depends_on_B_but_no_code_references_B is a package that depends on B, but source code does not reference any code from B.
     - linkage table should be empty
- D_depends_on_A_v1 is a normal package that depends on A v1, and source code references A.
     - linkage table should contain package A's ID (and the related upgrade info)
- D_depends_on_A_v1_but_no_code_references_B is a package that depends on A v1, but source code does not reference any code from A.
     - linkage table should be empty


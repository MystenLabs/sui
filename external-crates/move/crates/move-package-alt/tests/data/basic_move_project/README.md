This directory contains minimal Move projects with only a `Move.toml` file that has a list of environments and a list of dependencies.


- graph: it depends on `nodeps` and `depends_a_b` as local dependencies. Transitively, this depends on `pkg_a` and `pkg_b`.
- nodeps: just a toml, no deps
- pkg_a: just a toml, no deps
- pkg_b: just a toml, no deps
- depends_a_b: it depends on `pkg_a` and `pkg_b` as local dependencies
- pkg_git: is a template with 3 Move.toml files that is used to create a git repository with 3 commits. This allows for testing fetching git dependencies at different shas.
- pkg_dep_on_git: it depends on `pkg_git` for testing `update-deps` and other git fetching functionality

This directory contains minimal Move projects with only a `Move.toml` file that has a list of environments and a list of dependencies.


- graph: it depends on `nodeps` and `depends_a_b` as local dependencies. Transitively, this depends on `pkg_a` and `pkg_b`.
- nodeps: just a toml, no deps
- pkg_a: just a toml, no deps
- pkg_b: just a toml, no deps
- depends_a_b: it depends on `pkg_a` and `pkg_b` as local dependencies

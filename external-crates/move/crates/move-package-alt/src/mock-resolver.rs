//! A mock resolver that simply returns the data passed to it as a dependency resolution

use std::{
    collections::BTreeMap,
    env,
    io::{read_to_string, stdin},
};

use external_resolver::{QueryID, QueryResult, Request, Response, RESOLVE_ARG};
use serde::Deserialize;

pub fn main() {
    let args: Vec<String> = env::args().collect();

    assert!(
        args.len() == 2 && args[1] == RESOLVE_ARG,
        "External resolver must be called with a single argument `{RESOLVE_ARG}`"
    );

    let stdin = read_to_string(stdin()).expect("Stdin can be read");

    eprintln!("==== STDIN ===\n{stdin}\n==== END STDIN ====");
    let request: Request = toml::from_str(&stdin)
        .expect("External resolver must be passed a TOML-formatted request on stdin");

    eprintln!("Resolving for flavor {}", request.flavor);

    let responses: BTreeMap<QueryID, QueryResult> = request
        .queries
        .into_iter()
        .map(|(id, query)| {
            let env_str = match query.environment_id {
                Some(e) => format!("for environment {}", &e),
                None => "for default environment".to_string(),
            };

            eprintln!("Resolving request `{}` {env_str}.", id);

            (
                id,
                QueryResult::deserialize(query.argument)
                    .expect("Argument to mock resolver is expected to be a query result"),
            )
        })
        .collect();

    let response = Response { responses };

    println!(
        "{}",
        toml::to_string(&response).expect("response can be serialized")
    );
}

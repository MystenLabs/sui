// build.rs file
fn main() {
    let schema_name = "rpc";
    sui_graphql_client_build::register_schema(schema_name);
}

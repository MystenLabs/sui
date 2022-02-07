#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct AuthorityCommitteeConfig {
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    authority_configs: BTreeMap<FastPayAddress, AuthorityClientConfig>,
}
impl AuthorityCommitteeConfig {
    pub fn get_committee(&self) -> Committee {}
}
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
struct AuthorityClientConfig {
    pub address: FastPayAddress,
    pub host: String,
    pub base_port: u32,
    pub voting_rights: usize,
}

fn make_remote_authority_clients(
    authority_committee_config: &AuthorityCommitteeConfig,
) -> BTreeMap<AuthorityName, crate::authority_client::AuthorityClient> {
    let mut authority_clients = BTreeMap::new();
    for (name, config) in &authority_committee_config.authority_configs {
        let config = config.clone();
        let client = crate::authority_client::AuthorityClient::new(
            fastx_network::network::NetworkClient::new(
                config.host,
                config.base_port,
                authority_committee_config.buffer_size,
                authority_committee_config.send_timeout,
                authority_committee_config.recv_timeout,
            ),
        );
        authority_clients.insert(config.address, client);
    }
    authority_clients
}

/// Create a new client from a configutation, and store at given path
pub fn new_from_config(
    path: PathBuf,
    address: FastPayAddress,
    secret: KeyPair,
    remote_authority_committee_config: &AuthorityCommitteeConfig,
) -> Result<Self, FastPayError> {
    let auth_clients = make_remote_authority_clients(remote_authority_committee_config);
    ClientState::new(
        path,
        address,
        secret,
        committee,
        auth_clients,
        BTreeMap::new(),
        BTreeMap::new(),
    )
}

// pub fn load<A>(path: PathBuf) -> Result<Self, FastPayError> {
//     let client_state = ClientState {
//         address,
//         secret,
//         authorities: AuthorityAggregator::new(committee, authority_clients),
//         store: ClientStore::new(path),
//     };

//     // Backfill the DB
//     client_state.store.populate(object_refs, certificates)?;
//     Ok(client_state)
// }

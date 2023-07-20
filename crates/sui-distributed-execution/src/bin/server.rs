use std::collections::HashMap;
use std::fs;
use std::hash::Hash;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use clap::*;
use serde::Deserialize;
// use serde_json::Result;
use tokio::sync::mpsc;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use sui_distributed_execution::network_agents::{*, self};

const FILE_PATH:&str = "/Users/tonyzhang/Documents/UMich2023su/sui.nosync/crates/sui-distributed-execution/src/configs/config.json";

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(long)]
    pub my_id: UniqueId
}

#[derive(Clone, Deserialize, Debug)]
struct AppConfig {
    kind: String,
    ip_addr: IpAddr,
    port: u16,
    attrs: HashMap<String, String>,
}

fn init_agent<T: Agent>(id: UniqueId, conf: AppConfig) 
    -> (T, 
        mpsc::Sender<NetworkMessage>,
        mpsc::Receiver<NetworkMessage>,) 
{
    let (in_send, mut in_recv) = mpsc::channel(100);
    let (out_send, mut out_recv) = mpsc::channel(100);
    let agent = Agent::new(
            id,
            in_recv,
            out_send,
            conf.attrs
        );
    return (agent, in_send, out_recv)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Parse config from json
    let config_json = fs::read_to_string(FILE_PATH)
        .expect("Failed to read config file");
    let config: HashMap<UniqueId, AppConfig> 
        = serde_json::from_str(&config_json).unwrap();   

    // Initialize map from id to address
    let mut addr_table: HashMap<UniqueId, (IpAddr, u16)> = HashMap::new();
    for (id, entry) in &config {
        assert!(!addr_table.contains_key(&id), "ids must be unique");
        addr_table.insert(*id, (entry.ip_addr, entry.port));
    }

    // Parse command line
    let args = Args::parse();
    let my_id = args.my_id;
    assert!(config.contains_key(&my_id), "agent {} not in config", &my_id);

    // Initialize the agent
    let (mut agent, in_sender, out_receiver) = 
        init_agent::<EchoAgent>(my_id, (*config.get(&my_id).unwrap()).clone());

    // Initialize and run the network
    let network_manager = NetworkManager::new(
        my_id, addr_table, 
        in_sender, 
        out_receiver);

    
    tokio::spawn(async move {
        network_manager.run().await;
    });

    agent.run().await;
}



/*****************************************************************************************
 *                                     Network Manager                                   *
 *****************************************************************************************/

// Network Manager spawns and manages TCP connections

struct NetworkManager {
    my_id: UniqueId,
    my_addr: IpAddr,    // listening addr
    my_port: u16,       // listening port
    addr_table: HashMap<UniqueId, (IpAddr, u16)>,
    application_in: mpsc::Sender<NetworkMessage>,     // incoming messages for server
    application_out: mpsc::Receiver<NetworkMessage>,  // outgoing messages from server
}

impl NetworkManager {
    fn new(
        my_id: UniqueId,
        addr_table: HashMap<UniqueId, (IpAddr, u16)>,
        application_in: mpsc::Sender<NetworkMessage>,
        application_out: mpsc::Receiver<NetworkMessage>
    ) -> Self {
        NetworkManager {
            my_id,
            my_addr: addr_table.get(&my_id).unwrap().0,
            my_port: addr_table.get(&my_id).unwrap().1,
            addr_table,
            application_in,
            application_out,
        }
    }

    async fn handle_connection(my_id:UniqueId, socket: TcpStream, in_sender: mpsc::Sender<NetworkMessage>) {
        // TODO
        // Send my id, and receive id to update routing table.
        // Routing table can be Arc<RwLock>
        
    }

    async fn run(&self) {
        // Initialize connections
        let listener_address = SocketAddr::new(self.my_addr, self.my_port);
        let (in_sender, mut in_receiver) = mpsc::channel::<NetworkMessage>(100);
        let in_sender_clone = in_sender.clone();
        let my_id = self.my_id.clone();
        // TODO: in_send (orange) gets cloned to each TCP task
        // Each TCP task also has a (out_send, out_recv)
        // out_send is placed in routing table. Task polls out_recv and sends on TCP.

        // Listen for incoming connections
        tokio::spawn(async move {
            let listener = TcpListener::bind(listener_address).await.unwrap();
            println!("Server {} listening on {}", my_id, listener_address);
            
            // Accept incoming connections and spawn a task to handle each one
            while let Ok((socket, _)) = listener.accept().await {
                println!("Server {} accepted connection from: {}", my_id, socket.peer_addr().unwrap());
                let in_sender_clone = in_sender_clone.clone();
                // let (out_sender, mut out_receiver) = mpsc::channel::<NetworkMessage>(100);
                // out_sender goes into routing table
                tokio::spawn(async move {
                    println!("hello");
                    Self::handle_connection(my_id, socket, in_sender_clone).await;
                });
            }
        });

        // Connect to servers with lower id
        for (&id, &(ip, port)) in &self.addr_table {
            if id < self.my_id {
                let remote = SocketAddr::new(ip, port);
                println!("Server {} trying connect to {}", my_id, remote);
                let socket = TcpStream::connect(remote).await.unwrap();
                println!("Server {} connected to {}", my_id, remote);
                let in_sender_clone = in_sender.clone();

                tokio::spawn(async move {
                    Self::handle_connection(my_id, socket, in_sender_clone).await;
                });
            }
        }

        // Receive loop from TCP connections
        while let Some(msg) = in_receiver.recv().await {
            println!("Network manager received from agent {}:\n\t{}", msg.src, msg.payload);
            // push to application_in
        }
    }
}
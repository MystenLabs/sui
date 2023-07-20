use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, RwLock};
use std::net::{IpAddr, SocketAddr};
use clap::*;
use serde::Deserialize;
use tokio::io::{BufReader, AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use sui_distributed_execution::network_agents::*;

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

fn init_agent(id: UniqueId, conf: AppConfig) 
    -> (Box<dyn Agent>,
        mpsc::Sender<NetworkMessage>,
        mpsc::Receiver<NetworkMessage>,) 
{
    let (in_send, in_recv) = mpsc::channel(100);
    let (out_send, out_recv) = mpsc::channel(100);
    let agent: Box<dyn Agent> = match conf.kind.as_str() {
        "echo" => Box::new(EchoAgent::new(
            id,
            in_recv,
            out_send,
            conf.attrs
        )),
        "ping" => Box::new(PingAgent::new(
            id,
            in_recv,
            out_send,
            conf.attrs
        )),
        _ => {panic!("Invalid agent kind {}", conf.kind); }
    };
    
    return (agent, in_send, out_recv);
}

#[tokio::main()]
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
        init_agent(my_id, (*config.get(&my_id).unwrap()).clone());

    // Initialize and run the network
    let mut network_manager = NetworkManager::new(
        my_id, addr_table, 
        in_sender, 
        out_receiver);

    
    tokio::spawn(async move {
        network_manager.run().await;
    });

    // Wait for connections to be set up
    sleep(Duration::from_millis(1_000)).await;
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

    async fn handle_connection(my_id:UniqueId, socket: TcpStream, 
        out_sender: mpsc::Sender<NetworkMessage>,  // To be placed in routing table after handshake
        in_sender: mpsc::Sender<NetworkMessage>, out_receiver: &mut mpsc::Receiver<NetworkMessage>,
        routing_table: Arc<RwLock<HashMap<UniqueId, mpsc::Sender<NetworkMessage>>>>) 
    {
        println!("Handle connection");
        let mut stream = BufReader::new(socket);

        // First perform handshake. Send my id, and receive id to update routing table.
        let msg = format!("{}\n", my_id);
        stream.write_all(msg.as_bytes()).await.unwrap();
        println!("sending {}", msg);

        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        let remote_id = line.trim().parse().unwrap();
        println!("GOT {:?}", remote_id);


        {
            let mut w_routing_table = routing_table.write().unwrap();
            w_routing_table.insert(remote_id, out_sender);
        }

        loop {
            line = String::new();
            tokio::select! {
                Some(message) = out_receiver.recv() => {
                    println!("NetManager from application: {:?}", message);
                    let serialized = message.serialize();
                    stream.write_all(serialized.as_bytes()).await.expect("send failed");
                }
                Ok(_) = stream.read_line(&mut line) => {
                    println!("NetManager from tcp: {:?}", line);
                    if line.len() == 0 {
                        panic!("Connection with remote id {remote_id} broken");
                    }
                    let message = NetworkMessage::deserialize(line);
                    in_sender.send(message).await.expect("send failed");
                }
            }
        }
    }

    async fn run(&mut self) {
        // Initialize connections
        let listener_address = SocketAddr::new(self.my_addr, self.my_port);
        
        // Channel from link handlers to Network Manager
        let (in_sender, mut in_receiver) = mpsc::channel::<NetworkMessage>(100);

        let in_sender_clone = in_sender.clone();
        let my_id = self.my_id.clone();
        // TODO: in_send (orange) gets cloned to each TCP task
        // Each TCP task also has a (out_send, out_recv)
        // out_send is placed in routing table. Task polls out_recv and sends on TCP.

        let routing_table = Arc::new(RwLock::new(
                HashMap::<UniqueId, mpsc::Sender<NetworkMessage>>::new()
        ));
        let routing_table_clone = routing_table.clone();

        // Listen for incoming connections
        tokio::spawn(async move {
            let listener = TcpListener::bind(listener_address).await.unwrap();
            println!("Server {} listening on {}", my_id, listener_address);
            
            // Accept incoming connections and spawn a task to handle each one
            while let Ok((socket, _)) = listener.accept().await {
                println!("Server {} accepted connection from: {}", my_id, socket.peer_addr().unwrap());
                let in_sender_clone = in_sender_clone.clone();
                let routing_table_clone = routing_table_clone.clone();

                // Channel from Network Manager to link handler
                let (out_sender, mut out_receiver) = mpsc::channel::<NetworkMessage>(100);
                tokio::spawn(async move {
                             // Channel from link handlers to Network Manager
                    Self::handle_connection(my_id, socket, out_sender, in_sender_clone, &mut out_receiver, routing_table_clone).await;
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
                let routing_table_clone = routing_table.clone();

                // Channel from Network Manager to link handler
                let (out_sender, mut out_receiver) = mpsc::channel::<NetworkMessage>(100);

                tokio::spawn(async move {
                    Self::handle_connection(my_id, socket, out_sender, in_sender_clone, &mut out_receiver, routing_table_clone).await;
                });
            }
        }

        // Receive loop from TCP connections

        loop {
            tokio::select! {
                Some(message) = in_receiver.recv() => {
                    println!("NetManager from tcp: {:?}", message);
                    self.application_in.send(message).await.expect("send failed");
                }
                Some(message) = self.application_out.recv() => {
                    println!("NetManager from application: {:?}", message);
                    let dst = message.dst;
                    let out_chan: mpsc::Sender<NetworkMessage>;
                    {
                        let r_routing_table = routing_table.read().unwrap();
                        out_chan = r_routing_table.get(&dst).unwrap().clone();
                    }
                    out_chan.send(message).await.expect("send failed");
                }
            }
        }
        // Select from in_receiver.recv() or application_out
    }
}
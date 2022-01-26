mod cli_pretty;
mod client_api;
mod server_api;

use std::thread;
use std::time::Duration;

use fastpay::{
    config::{AccountsConfig, AuthorityServerConfig, CommitteeConfig},
    transport,
};

fn main() {
    let num_servers: usize = 4;
    let send_timeout = Duration::from_micros(10000000000);
    let recv_timeout = Duration::from_micros(10000000000);

    // Create some account config container
    let mut acc_cfg = AccountsConfig::new();
    // Committee config is an aggregate of auth configs
    let mut committee_cfg = CommitteeConfig::new();

    // Generate configs for the servers
    let mut auth_serv_cfgs = Vec::new();
    for i in 0..num_servers {
        let db_dir = "db".to_owned() + &i.to_string();
        let s = server_api::create_server_configs(
            "127.0.0.1".to_string(),
            (9100 + i).try_into().unwrap(),
            db_dir,
        );
        // let auth
        let auth = AuthorityServerConfig {
            authority: s.authority.clone(),
            key: s.key.copy(),
        };
        auth_serv_cfgs.push(auth);

        committee_cfg.authorities.push(s.authority.clone());
    }

    // Create accounts with starting values
    let initial_state_cfg = client_api::create_account_configs(
        &mut acc_cfg, 
        5, 
        20000, 
        3);

    let acc1 = initial_state_cfg.config.get(0).unwrap().address;
    let acc2 = initial_state_cfg.config.get(1).unwrap().address;

    let buffer_size: usize = transport::DEFAULT_MAX_DATAGRAM_SIZE
        .to_string()
        .parse()
        .unwrap();

    // Thread handles for servers
    let mut thrs = Vec::new();

    // Run the servers with the init values
    for i in 0..num_servers {
        let s = auth_serv_cfgs.get(i).unwrap();
        let auth = AuthorityServerConfig {
            authority: s.authority.clone(),
            key: s.key.copy(),
        };
        let cfg = committee_cfg.clone();
        let init_cfg = initial_state_cfg.clone();

        println!(
            "Server {:?} running on {}:{}",
            s.authority.address, s.authority.host, s.authority.base_port
        );
        thrs.push(thread::spawn(move || {
            server_api::run_server(
                "0.0.0.0", 
                auth, 
                cfg, 
                init_cfg, 
                buffer_size)
        }));
    }

    println!("Sleeping 3 secs. Waiting for servers to startup");
    thread::sleep(Duration::from_secs(3));

    // Servers are running
    // Can start interacting

    // Get the objects for acc1 and acc2
    let acc1_objs = client_api::get_account_objects(
        acc1,
        send_timeout,
        recv_timeout,
        buffer_size,
        &mut acc_cfg,
        &committee_cfg,
    );
    let acc2_objs = client_api::get_account_objects(
        acc2,
        send_timeout,
        recv_timeout,
        buffer_size,
        &mut acc_cfg,
        &committee_cfg,
    );

    let acc1_obj1 = acc1_objs.get(0).unwrap().0;
    let acc1_gas = acc1_objs.get(1).unwrap().0;
    println!("{} {}", acc1_obj1, acc1_gas);

    let acc2_obj1 = acc2_objs.get(0).unwrap().0;

    let acc1_obj1 = client_api::get_object_info(
        acc1_obj1,
        &mut acc_cfg,
        &committee_cfg,
        send_timeout,
        recv_timeout,
        buffer_size,
    );

    let acc2_obj1 = client_api::get_object_info(
        acc2_obj1,
        &mut acc_cfg,
        &committee_cfg,
        send_timeout,
        recv_timeout,
        buffer_size,
    );

    // Transfer from ACC1 to ACC2
    client_api::transfer_object(
        acc1,
        acc2,
        acc1_obj1.object.id(),
        acc1_gas,
        &mut acc_cfg,
        &committee_cfg,
        send_timeout,
        recv_timeout,
        buffer_size,
    );

    // Fetch and pretty print the object states
    let o1 = client_api::get_object_info(
        acc1_obj1.object.id(),
        &mut acc_cfg,
        &committee_cfg,
        send_timeout,
        recv_timeout,
        buffer_size,
    );
    let o2 = client_api::get_object_info(
        acc2_obj1.object.id(),
        &mut acc_cfg,
        &committee_cfg,
        send_timeout,
        recv_timeout,
        buffer_size,
    );

    println!("Object States after transfer:\n");

    cli_pretty::format_obj_info_response(&o1).printstd();
    cli_pretty::format_obj_info_response(&o2).printstd();

    for thr in thrs {
        thr.join();
    }
}

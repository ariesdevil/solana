extern crate clap;
extern crate getopts;
extern crate log;
extern crate serde_json;
extern crate solana;

use clap::{App, Arg};
use solana::client::mk_client;
use solana::crdt::{NodeInfo, TestNode};
use solana::drone::DRONE_PORT;
use solana::fullnode::{Config, FullNode, LedgerFile};
use solana::logger;
use solana::metrics::set_panic_hook;
use solana::service::Service;
use solana::signature::{KeyPair, KeyPairUtil};
use solana::wallet::request_airdrop;
use std::fs::File;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::exit;
//use std::time::Duration;

fn main() -> () {
    logger::setup();
    set_panic_hook("fullnode");
    let matches = App::new("fullnode")
        .arg(
            Arg::with_name("identity")
                .short("i")
                .long("identity")
                .value_name("FILE")
                .takes_value(true)
                .help("run with the identity found in FILE"),
        )
        .arg(
            Arg::with_name("testnet")
                .short("t")
                .long("testnet")
                .value_name("HOST:PORT")
                .takes_value(true)
                .help("connect to the network at this gossip entry point"),
        )
        .arg(
            Arg::with_name("ledger")
                .short("L")
                .long("ledger")
                .value_name("FILE")
                .takes_value(true)
                .help("use FILE as persistent ledger (defaults to stdin/stdout)"),
        )
        .get_matches();

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8000);
    let mut keypair = KeyPair::new();
    let mut repl_data = NodeInfo::new_leader_with_pubkey(keypair.pubkey(), &bind_addr);
    if let Some(i) = matches.value_of("identity") {
        let path = i.to_string();
        if let Ok(file) = File::open(path.clone()) {
            let parse: serde_json::Result<Config> = serde_json::from_reader(file);
            if let Ok(data) = parse {
                keypair = data.keypair();
                repl_data = data.node_info;
            } else {
                eprintln!("failed to parse {}", path);
                exit(1);
            }
        } else {
            eprintln!("failed to read {}", path);
            exit(1);
        }
    }

    let leader_pubkey = keypair.pubkey();
    let repl_clone = repl_data.clone();

    let ledger = if let Some(l) = matches.value_of("ledger") {
        LedgerFile::Path(l.to_string())
    } else {
        LedgerFile::StdInOut
    };

    let mut node = TestNode::new_with_bind_addr(repl_data, bind_addr);
    let mut drone_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DRONE_PORT);
    let fullnode = if let Some(t) = matches.value_of("testnet") {
        let testnet_address_string = t.to_string();
        let testnet_addr: SocketAddr = testnet_address_string.parse().unwrap();
        drone_addr.set_ip(testnet_addr.ip());

        FullNode::new(node, false, ledger, keypair, Some(testnet_addr))
    } else {
        node.data.leader_id = node.data.id;

        FullNode::new(node, true, ledger, keypair, None)
    };

    let mut client = mk_client(&repl_clone);
    let previous_balance = client.poll_get_balance(&leader_pubkey).unwrap();
    eprintln!("balance is {}", previous_balance);

    if previous_balance == 0 {
        eprintln!("requesting airdrop from {}", drone_addr);
        request_airdrop(&drone_addr, &leader_pubkey, 50).unwrap_or_else(|_| {
            panic!(
                "Airdrop failed, is the drone address correct {:?} drone running?",
                drone_addr
            )
        });

        let balance = client.poll_get_balance(&leader_pubkey).unwrap();
        eprintln!("new balance is {}", balance);
        assert!(balance > 0);
    }

    fullnode.join().expect("join");
}

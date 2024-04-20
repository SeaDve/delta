use std::time::Duration;

use libp2p::{futures::StreamExt, ping, swarm::SwarmEvent, Multiaddr, SwarmBuilder};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_behaviour(|_| {
            ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(1)))
        })
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build();

    swarm
        .listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse().unwrap())
        .unwrap();

    if let Some(addr) = std::env::args().nth(1) {
        let remote = addr.parse::<Multiaddr>().unwrap();
        swarm.dial(remote).unwrap();
        tracing::info!("Dialed {addr}")
    }

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => tracing::info!("Listening on {address:?}"),
            SwarmEvent::Behaviour(event) => tracing::info!("{event:?}"),
            _ => {}
        }
    }
}

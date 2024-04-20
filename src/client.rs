use std::time::Duration;

use anyhow::Result;
use futures_channel::oneshot;
use futures_util::{FutureExt, StreamExt};
use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use libp2p::{
    gossipsub, mdns,
    swarm::{NetworkBehaviour, SwarmEvent},
    PeerId, SwarmBuilder,
};

mod imp {
    use std::sync::OnceLock;

    use gtk::glib::subclass::Signal;

    use super::*;

    #[derive(Default)]
    pub struct Client {
        pub(super) command_tx: OnceLock<async_channel::Sender<Command>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Client {
        const NAME: &'static str = "DeltaClient";
        type Type = super::Client;
    }

    impl ObjectImpl for Client {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            glib::spawn_future_local(clone!(@weak obj => async move {
                if let Err(err) = obj.init().await {
                    tracing::error!("Failed to initialize client: {:?}", err);
                }
            }));
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![Signal::builder("message-received")
                    .param_types([String::static_type()])
                    .build()]
            })
        }
    }
}

glib::wrapper! {
    pub struct Client(ObjectSubclass<imp::Client>);
}

impl Client {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_message_received<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &str) + 'static,
    {
        self.connect_closure(
            "message-received",
            false,
            closure_local!(|obj: &Self, message: &str| {
                f(obj, message);
            }),
        )
    }

    pub async fn send_message(&self, message: &str) {
        self.send_command(Command::SendMessage {
            message: message.to_string(),
        })
        .await;
    }

    pub async fn list_peers(&self) -> Vec<PeerId> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::ListPeers { tx }).await;
        rx.await.unwrap()
    }

    async fn send_command(&self, command: Command) {
        let imp = self.imp();

        let command_tx = imp.command_tx.get().unwrap();
        command_tx.send(command).await.unwrap();
    }

    async fn init(&self) -> Result<()> {
        let imp = self.imp();

        let (command_tx, command_rx) = async_channel::bounded(1);
        imp.command_tx.set(command_tx).unwrap();

        let mut swarm = SwarmBuilder::with_new_identity()
            .with_async_std()
            .with_quic()
            .with_behaviour(|key| {
                let gossipsub_config = gossipsub::ConfigBuilder::default().build()?;

                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )?;

                let mdns = mdns::async_io::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )?;

                Ok(MyBehaviour { gossipsub, mdns })
            })?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
            })
            .build();

        let topic = gossipsub::IdentTopic::new("delta");
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

        loop {
            futures_util::select! {
                command = command_rx.recv().fuse() => {
                    match command {
                        Ok(Command::SendMessage { message }) => {
                            if let Err(err) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), message) {
                                tracing::error!("Failed to send message: {:?}", err);
                            }
                        }
                        Ok(Command::ListPeers {tx}) => {
                            let peers = swarm.connected_peers().copied().collect::<Vec<_>>();
                            tx.send(peers).unwrap();
                        }
                        Err(err) => {
                            tracing::error!("Failed to receive command: {:?}", err);
                            break;
                        }
                    }
                }
                event = swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer_id, _multiaddr) in list {
                            tracing::debug!("mDNS discovered a new peer: {peer_id}");
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                        }
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                        for (peer_id, _multiaddr) in list {
                            tracing::debug!("mDNS discover peer has expired: {peer_id}");
                            swarm
                                .behaviour_mut()
                                .gossipsub
                                .remove_explicit_peer(&peer_id);
                        }
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    })) => {
                        let message_str = String::from_utf8_lossy(&message.data);

                        tracing::debug!(
                            "Got message: '{}' with id: {id} from peer: {peer_id}",
                            message_str,
                        );

                        self.emit_by_name::<()>("message-received", &[&message_str.to_string()]);
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        tracing::debug!("Local node is listening on {address}");
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

enum Command {
    SendMessage { message: String },
    ListPeers { tx: oneshot::Sender<Vec<PeerId>> },
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::async_io::Behaviour,
}

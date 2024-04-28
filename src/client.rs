use std::time::Duration;

use anyhow::{anyhow, ensure, Context, Result};
use futures_channel::oneshot;
use futures_util::{select, FutureExt, StreamExt};
use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use libp2p::{
    gossipsub, mdns,
    swarm::{NetworkBehaviour, SwarmEvent},
    PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use libp2p_stream as stream;
use serde::{Deserialize, Serialize};

use crate::{
    call::{Call, CallState},
    config,
    input_stream::InputStream,
    output_stream::OutputStream,
    peer::Peer,
    peer_list::PeerList,
};

const AUDIO_STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/audio");

#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "DeltaMessageReceived")]
pub struct MessageReceived {
    pub source: PeerId,
    pub message: String,
}

mod imp {
    use std::{cell::RefCell, sync::OnceLock};

    use gtk::glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Client)]
    pub struct Client {
        #[property(get)]
        pub(super) active_call: RefCell<Option<Call>>,

        pub(super) command_tx: OnceLock<async_channel::Sender<Command>>,
        pub(super) call_incoming_response_tx:
            RefCell<Option<oneshot::Sender<CallIncomingResponse>>>,
        pub(super) call_incoming_cancel_tx: RefCell<Option<oneshot::Sender<()>>>,

        pub(super) peer_list: PeerList,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Client {
        const NAME: &'static str = "DeltaClient";
        type Type = super::Client;
    }

    #[glib::derived_properties]
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
                    .param_types([MessageReceived::static_type()])
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
        F: Fn(&Self, &MessageReceived) + 'static,
    {
        self.connect_closure(
            "message-received",
            false,
            closure_local!(|obj: &Self, message: &MessageReceived| f(obj, message)),
        )
    }

    pub fn peer_list(&self) -> &PeerList {
        &self.imp().peer_list
    }

    pub async fn publish_message(&self, message: &str, destination: MessageDestination) {
        self.publish(PublishData::Message {
            message: message.to_string(),
            destination,
        })
        .await;
    }

    pub async fn call_request(&self, destination: PeerId) -> Result<()> {
        ensure!(self.active_call().is_none(), "Already in a call");

        self.publish(PublishData::CallRequest { destination }).await;

        let destination_peer = self.peer_list().get(&destination).unwrap();
        let call = Call::new(&destination_peer);
        call.set_state(CallState::Outgoing);

        self.set_active_call(Some(call.clone()));

        Ok(())
    }

    pub async fn call_outgoing_cancel(&self) -> Result<()> {
        let active_call = self
            .active_call()
            .context("No active outgoing call to cancel")?;
        debug_assert_eq!(active_call.state(), CallState::Outgoing);

        active_call.set_state(CallState::Ended);

        self.publish(PublishData::CallRequestCancel {
            destination: *active_call.peer().id(),
        })
        .await;

        self.set_active_call(None);

        Ok(())
    }

    pub fn call_incoming_accept(&self) {
        debug_assert_eq!(
            self.active_call().map(|c| c.state()),
            Some(CallState::Incoming)
        );

        let imp = self.imp();
        let tx = imp.call_incoming_response_tx.take().unwrap();
        tx.send(CallIncomingResponse::Accept).unwrap();
    }

    pub fn call_incoming_decline(&self) {
        debug_assert_eq!(
            self.active_call().map(|c| c.state()),
            Some(CallState::Incoming)
        );

        let imp = self.imp();
        let tx = imp.call_incoming_response_tx.take().unwrap();
        tx.send(CallIncomingResponse::Reject).unwrap();
    }

    fn set_active_call(&self, call: Option<Call>) {
        self.imp().active_call.replace(call);
        self.notify_active_call();
    }

    async fn publish(&self, data: PublishData) {
        tracing::debug!("Publishing data: {:?}", data);

        self.send_command(Command::Publish(data)).await;
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
                    mdns::Config {
                        ttl: Duration::from_secs(2),
                        query_interval: Duration::from_secs(1),
                        ..Default::default()
                    },
                    key.public().to_peer_id(),
                )?;

                let stream = stream::Behaviour::new();

                Ok(Behaviour {
                    gossipsub,
                    mdns,
                    stream,
                })
            })?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::MAX))
            .build();

        tracing::debug!("Local peer id: {peer_id}", peer_id = swarm.local_peer_id());

        let topic = gossipsub::IdentTopic::new("delta");
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

        let mut incoming_streams = swarm
            .behaviour()
            .stream
            .new_control()
            .accept(AUDIO_STREAM_PROTOCOL)?;

        glib::spawn_future_local(clone!(@weak self as obj => async move {
            while let Some((their_peer_id, output_stream)) = incoming_streams.next().await {
                tracing::debug!("Incoming stream from {}", their_peer_id);

                if let Some(active_call) = obj.active_call() {
                    if active_call.peer().id() == &their_peer_id {
                        if let Err(err) = active_call.set_output_stream(OutputStream::new(output_stream))  {
                            tracing::error!("Failed to set output stream: {:?}", err);
                        }
                    } else {
                        tracing::warn!("Received stream from unexpected peer: {their_peer_id}");
                    }
                } else {
                    tracing::warn!("Received stream without active call");
                }
            }
        }));

        loop {
            futures_util::select! {
                command = command_rx.recv().fuse() => {
                    if let Err(err) = self.handle_command(&mut swarm, &topic, command?).await {
                        tracing::error!("Failed to handle command: {:?}", err);
                    }
                }
                event = swarm.select_next_some() => {
                    if let Err(err) = self.handle_swarm_event(&mut swarm, event).await {
                        tracing::error!("Failed to handle swarm event: {:?}", err);
                    }
                }
            }
        }
    }

    // Handle outgoing commands
    async fn handle_command(
        &self,
        swarm: &mut Swarm<Behaviour>,
        topic: &gossipsub::IdentTopic,
        command: Command,
    ) -> Result<()> {
        match command {
            Command::Publish(data) => {
                let data_bytes = serde_json::to_vec(&data)?;
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic.clone(), data_bytes)?;
            }
        }

        Ok(())
    }

    // Handle ingoing events
    async fn handle_swarm_event(
        &self,
        swarm: &mut Swarm<Behaviour>,
        event: SwarmEvent<BehaviourEvent>,
    ) -> Result<()> {
        let imp = self.imp();

        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                for (peer_id, _multiaddr) in list {
                    tracing::trace!("mDNS discovered a new peer: {peer_id}");

                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    self.peer_list().insert(Peer::new(peer_id));
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                for (peer_id, _multiaddr) in list {
                    tracing::trace!("mDNS discover peer has expired: {peer_id}");

                    swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                    self.peer_list().remove(&peer_id);
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: their_peer_id,
                message: raw_message,
                ..
            })) => {
                tracing::debug!("received message from {}", their_peer_id);

                match serde_json::from_slice(&raw_message.data)? {
                    PublishData::Name { name } => {
                        if let Some(source_peer_id) = raw_message.source {
                            if let Some(peer) = self.peer_list().get(&source_peer_id) {
                                peer.set_name(name);
                            } else {
                                tracing::warn!("Received name for unknown peer: {source_peer_id}")
                            }
                        } else {
                            tracing::warn!("Received name without source peer id");
                        }
                    }
                    PublishData::Message {
                        message,
                        destination,
                    } => {
                        let should_accept = match destination {
                            MessageDestination::All => true,
                            MessageDestination::Peers(ref peer_ids) => {
                                peer_ids.contains(swarm.local_peer_id())
                            }
                        };

                        if let Some(source_peer_id) = raw_message.source {
                            if should_accept {
                                let message_received = MessageReceived {
                                    source: source_peer_id,
                                    message,
                                };
                                self.emit_by_name::<()>("message-received", &[&message_received]);
                            }
                        } else {
                            tracing::warn!("Received message without source peer id");
                        }
                    }
                    PublishData::CallRequest { ref destination }
                        if destination == swarm.local_peer_id() =>
                    {
                        let (call_incoming_response_tx, mut call_incoming_response_rx) =
                            oneshot::channel();
                        imp.call_incoming_response_tx
                            .replace(Some(call_incoming_response_tx));

                        let (call_incoming_cancel_tx, mut call_incoming_cancel_rx) =
                            oneshot::channel();
                        imp.call_incoming_cancel_tx
                            .replace(Some(call_incoming_cancel_tx));

                        let peer = self.peer_list().get(&their_peer_id).unwrap();
                        let call = Call::new(&peer);
                        call.set_state(CallState::Incoming);

                        let had_active_call = self.active_call().is_some();

                        self.set_active_call(Some(call.clone()));

                        let mut stream_control = swarm.behaviour().stream.new_control();

                        // Spawn a task here so we don't block the loop while waiting for the response
                        glib::spawn_future_local(clone!(@weak self as obj => async move {
                            let response = select! {
                                response = call_incoming_response_rx => response.unwrap(),
                                _ = call_incoming_cancel_rx => CallIncomingResponse::Cancelled,
                            };

                            tracing::debug!("Received call request: {:?}", response);

                            if response == CallIncomingResponse::Accept && !had_active_call {
                                tracing::debug!("Opening output stream to {their_peer_id}");

                                let input_stream = match stream_control
                                    .open_stream(their_peer_id, AUDIO_STREAM_PROTOCOL)
                                    .await
                                {
                                    Ok(stream) => stream,
                                    Err(err) => {
                                        tracing::error!("Failed to open output stream: {:?}", err);
                                        return;
                                    }
                                };

                                if let Err(err) =
                                    call.set_input_stream(InputStream::new(input_stream))
                                {
                                    tracing::error!("Failed to set input stream: {:?}", err);
                                    return;
                                }

                                call.set_state(CallState::Connected);

                                obj.publish(PublishData::CallRequestResponse {
                                    destination: their_peer_id,
                                    response: CallRequestResponse::Accept,
                                })
                                .await;
                            } else {
                                if response != CallIncomingResponse::Cancelled {
                                    obj.publish(PublishData::CallRequestResponse {
                                        destination: their_peer_id,
                                        response: CallRequestResponse::Reject,
                                    })
                                    .await;
                                }

                                call.set_state(CallState::Ended);
                                obj.set_active_call(None);
                            }
                        }));
                    }
                    PublishData::CallRequestCancel { ref destination }
                        if destination == swarm.local_peer_id() =>
                    {
                        tracing::debug!("Received call request cancel: {their_peer_id}");

                        if self.active_call().is_some() {
                            let tx = imp.call_incoming_cancel_tx.take().unwrap();
                            tx.send(()).unwrap();
                        } else {
                            tracing::warn!("Received call request cancel without active call");
                        }
                    }
                    PublishData::CallRequestResponse {
                        ref destination,
                        response,
                    } if destination == swarm.local_peer_id() => {
                        tracing::debug!("Received call request response: {:?}", response);

                        match response {
                            CallRequestResponse::Accept => {
                                tracing::debug!("Opening output stream to {their_peer_id}");

                                let input_stream = swarm
                                    .behaviour()
                                    .stream
                                    .new_control()
                                    .open_stream(their_peer_id, AUDIO_STREAM_PROTOCOL)
                                    .await
                                    .map_err(|err| anyhow!(err))?;

                                let active_call = self.active_call().unwrap();
                                active_call.set_input_stream(InputStream::new(input_stream))?;
                                active_call.set_state(CallState::Connected);
                            }
                            CallRequestResponse::Reject => {
                                let active_call = self.active_call().unwrap();
                                active_call.set_state(CallState::Ended);
                                self.set_active_call(None);
                            }
                            CallRequestResponse::Cancelled => unreachable!(),
                        }
                    }
                    other_published_data => {
                        tracing::debug!("Ignoring published data: {:?}", other_published_data);
                    }
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Subscribed {
                ..
            })) => {
                self.publish(PublishData::Name {
                    name: config::name(),
                })
                .await;
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::trace!("Local node is listening on {address}");
            }
            _ => {
                tracing::trace!("Unhandled swarm event: {:?}", event);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageDestination {
    All,
    Peers(Vec<PeerId>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CallRequestResponse {
    Accept,
    Reject,
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
enum CallIncomingResponse {
    Accept,
    Reject,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
enum PublishData {
    Name {
        name: String,
    },
    Message {
        destination: MessageDestination,
        message: String,
    },
    CallRequest {
        destination: PeerId,
    },
    CallRequestCancel {
        destination: PeerId,
    },
    CallRequestResponse {
        destination: PeerId,
        response: CallRequestResponse,
    },
}

enum Command {
    Publish(PublishData),
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::async_io::Behaviour,
    stream: stream::Behaviour,
}

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
    call::{Call, CallEndReason, CallState},
    config,
    input_stream::InputStream,
    location::Location,
    output_stream::OutputStream,
    peer::Peer,
    peer_list::PeerList,
    wireless_info::SignalQuality,
    Application,
};

const PUBLISH_PROPERTIES_INTERVAL: Duration = Duration::from_secs(3);

const AUDIO_STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/audio");

#[derive(Debug, Clone, Copy, Serialize, Deserialize, glib::Enum)]
#[enum_type(name = "DeltaAlertType")]
pub enum AlertType {
    Sos,
    Hazard,
    Yielding,
}

impl AlertType {
    pub fn blink_count(self) -> u32 {
        match self {
            AlertType::Sos => 10,
            AlertType::Hazard => 5,
            AlertType::Yielding => 3,
        }
    }
}

mod imp {
    use std::{
        cell::{OnceCell, RefCell},
        sync::OnceLock,
    };

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Client)]
    pub struct Client {
        #[property(get)]
        pub(super) active_call: RefCell<Option<Call>>,

        pub(super) command_tx: OnceCell<async_channel::Sender<Command>>,
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

            glib::spawn_future_local(clone!(
                #[weak]
                obj,
                async move {
                    if let Err(err) = obj.init().await {
                        tracing::error!("Failed to initialize client: {:?}", err);
                    }
                }
            ));

            let app = Application::get();

            let gps = app.gps();
            gps.connect_location_notify(clone!(
                #[weak]
                obj,
                move |gps| {
                    let location = gps.location();
                    glib::spawn_future_local(async move {
                        obj.publish(PublishData::PropertyChanged(vec![Property::Location(
                            location,
                        )]))
                        .await;
                    });
                }
            ));
            gps.connect_speed_notify(clone!(
                #[weak]
                obj,
                move |gps| {
                    let speed = gps.speed();
                    glib::spawn_future_local(async move {
                        obj.publish(PublishData::PropertyChanged(vec![Property::Speed(speed)]))
                            .await;
                    });
                }
            ));

            app.settings().connect_icon_name_notify(clone!(
                #[weak]
                obj,
                move |settings| {
                    let icon_name = settings.icon_name();
                    glib::spawn_future_local(async move {
                        obj.publish(PublishData::PropertyChanged(vec![Property::IconName(
                            icon_name,
                        )]))
                        .await;
                    });
                }
            ));

            app.wireless_info().connect_signal_quality_notify(clone!(
                #[weak]
                obj,
                move |wireless_info| {
                    let signal_quality = wireless_info.signal_quality();
                    glib::spawn_future_local(async move {
                        obj.publish(PublishData::PropertyChanged(vec![Property::SignalQuality(
                            signal_quality,
                        )]))
                        .await;
                    });
                }
            ));
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![Signal::builder("alert-received")
                    .param_types([Peer::static_type(), AlertType::static_type()])
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

    pub fn connect_alert_received<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Peer, AlertType) + 'static,
    {
        self.connect_closure(
            "alert-received",
            false,
            closure_local!(|obj: &Self, peer: &Peer, alert_type: AlertType| f(
                obj, peer, alert_type
            )),
        )
    }

    pub fn peer_list(&self) -> &PeerList {
        &self.imp().peer_list
    }

    pub async fn publish_alert(&self, alert_type: AlertType) {
        self.publish(PublishData::Alert(alert_type)).await;
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

    pub async fn call_outgoing_cancel(&self) -> Result<()> {
        let active_call = self
            .active_call()
            .context("No active outgoing call to cancel")?;
        debug_assert_eq!(active_call.state(), CallState::Outgoing);

        active_call.set_state(CallState::Ended(CallEndReason::Other));

        self.publish(PublishData::CallRequestCancel {
            destination: *active_call.peer().id(),
        })
        .await;

        Ok(())
    }

    pub fn call_ongoing_end(&self) -> Result<()> {
        let active_call = self
            .active_call()
            .context("No active ongoing call to end")?;
        debug_assert_eq!(active_call.state(), CallState::Ongoing);

        active_call.start_end();

        Ok(())
    }

    fn set_active_call(&self, call: Option<Call>) {
        if let Some(ref call) = call {
            call.connect_state_notify(clone!(
                #[weak(rename_to = obj)]
                self,
                move |call| {
                    if matches!(call.state(), CallState::Ended(_)) {
                        obj.set_active_call(None);
                    }
                }
            ));
        }

        self.imp().active_call.replace(call);
        self.notify_active_call();
    }

    async fn publish(&self, data: PublishData) {
        tracing::trace!("Publishing data: {:?}", data);

        self.send_command(Command::Publish(data)).await;
    }

    async fn publish_properties(&self) {
        let app = Application::get();

        let gps = app.gps();
        let location = gps.location();
        let speed = gps.speed();

        let icon_name = app.settings().icon_name();

        let signal_quality = app.wireless_info().signal_quality();

        self.publish(PublishData::PropertyChanged(vec![
            Property::Name(config::name()),
            Property::Location(location),
            Property::Speed(speed),
            Property::SignalQuality(signal_quality),
            Property::IconName(icon_name),
        ]))
        .await;
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
                        ttl: Duration::from_secs(5),
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

        glib::spawn_future_local(clone!(
            #[weak(rename_to = obj)]
            self,
            async move {
                while let Some((their_peer_id, output_stream)) = incoming_streams.next().await {
                    tracing::debug!("Incoming stream from {}", their_peer_id);

                    if let Some(active_call) = obj.active_call() {
                        if active_call.peer().id() == &their_peer_id {
                            if let Err(err) =
                                active_call.set_output_stream(OutputStream::new(output_stream))
                            {
                                tracing::error!("Failed to set output stream: {:?}", err);
                            }
                        } else {
                            tracing::warn!("Received stream from unexpected peer: {their_peer_id}");
                        }
                    } else {
                        tracing::warn!("Received stream without active call");
                    }
                }
            }
        ));

        // Periodically publish our properties as a workaround to missing packets on unreliable networks
        glib::spawn_future_local(clone!(
            #[weak(rename_to = obj)]
            self,
            async move {
                loop {
                    glib::timeout_future(PUBLISH_PROPERTIES_INTERVAL).await;

                    obj.publish_properties().await;
                }
            }
        ));

        loop {
            select! {
                command = command_rx.recv().fuse() => {
                    if let Err(err) = self.handle_command(&mut swarm, &topic, command?) {
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
    fn handle_command(
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
                for (peer_id, multiaddr) in list {
                    tracing::trace!("mDNS discovered a new peer: {peer_id} at {multiaddr}");

                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    self.peer_list().insert(Peer::new(peer_id));
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                for (peer_id, multiaddr) in list {
                    tracing::trace!("mDNS discover peer has expired: {peer_id} at {multiaddr}");

                    swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                    self.peer_list().remove(&peer_id);
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: their_peer_id,
                message,
                ..
            })) => {
                tracing::debug!("received message from {}", their_peer_id);

                match serde_json::from_slice(&message.data)? {
                    PublishData::PropertyChanged(props) => {
                        let their_peer_id = message
                            .source
                            .context("Received property changed without unknown source")?;
                        let peer = self
                            .peer_list()
                            .get(&their_peer_id)
                            .context("Received property changed for unknown peer")?;
                        for prop in props {
                            match prop {
                                Property::Name(name) => {
                                    peer.set_name(name);
                                }
                                Property::Location(location) => {
                                    peer.set_location(location);
                                }
                                Property::Speed(speed) => {
                                    peer.set_speed(speed);
                                }
                                Property::SignalQuality(signal_quality) => {
                                    peer.set_signal_quality(signal_quality);
                                }
                                Property::IconName(icon_name) => {
                                    peer.set_icon_name(icon_name);
                                }
                            }
                        }
                    }
                    PublishData::CallRequest { ref destination }
                        if destination == swarm.local_peer_id() =>
                    {
                        if self.active_call().is_some() {
                            self.publish(PublishData::CallRequestResponse {
                                destination: their_peer_id,
                                response: CallRequestResponse::Reject(
                                    CallRequestRejectReason::AlreadyInCall,
                                ),
                            })
                            .await;

                            tracing::debug!(
                                "Rejected another call since a call is already in progress"
                            );

                            return Ok(());
                        }

                        let peer = self
                            .peer_list()
                            .get(&their_peer_id)
                            .context("Received call request for unknown peer")?;

                        if !Application::get().settings().is_allowed_peer(&peer.name()) {
                            self.publish(PublishData::CallRequestResponse {
                                destination: their_peer_id,
                                response: CallRequestResponse::Reject(
                                    CallRequestRejectReason::MutedByUser,
                                ),
                            })
                            .await;

                            tracing::debug!("Ignored call from muted peer");

                            return Ok(());
                        }

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

                        self.set_active_call(Some(call.clone()));

                        let mut stream_control = swarm.behaviour().stream.new_control();

                        // Spawn a task here so we don't block the loop while waiting for the response
                        glib::spawn_future_local(clone!(
                            #[weak(rename_to = obj)]
                            self,
                            async move {
                                let response = select! {
                                    response = call_incoming_response_rx => response.unwrap(),
                                    _ = call_incoming_cancel_rx => CallIncomingResponse::Cancelled,
                                };

                                tracing::debug!("Received call request: {:?}", response);

                                if response == CallIncomingResponse::Accept {
                                    tracing::debug!("Opening output stream to {their_peer_id}");

                                    let input_stream = match stream_control
                                        .open_stream(their_peer_id, AUDIO_STREAM_PROTOCOL)
                                        .await
                                    {
                                        Ok(stream) => stream,
                                        Err(err) => {
                                            tracing::error!(
                                                "Failed to open output stream: {:?}",
                                                err
                                            );
                                            return;
                                        }
                                    };

                                    if let Err(err) =
                                        call.set_input_stream(InputStream::new(input_stream))
                                    {
                                        tracing::error!("Failed to set input stream: {:?}", err);
                                        return;
                                    }

                                    call.set_state(CallState::Ongoing);

                                    obj.publish(PublishData::CallRequestResponse {
                                        destination: their_peer_id,
                                        response: CallRequestResponse::Accept,
                                    })
                                    .await;
                                } else {
                                    if response == CallIncomingResponse::Reject {
                                        obj.publish(PublishData::CallRequestResponse {
                                            destination: their_peer_id,
                                            response: CallRequestResponse::Reject(
                                                CallRequestRejectReason::RejectedByUser,
                                            ),
                                        })
                                        .await;
                                    }

                                    call.set_state(CallState::Ended(CallEndReason::Other));
                                }
                            }
                        ));
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
                                active_call.set_state(CallState::Ongoing);
                            }
                            CallRequestResponse::Reject(reject_reason) => {
                                let end_reason = match reject_reason {
                                    CallRequestRejectReason::AlreadyInCall => {
                                        CallEndReason::PeerInAnotherCall
                                    }
                                    CallRequestRejectReason::RejectedByUser => {
                                        CallEndReason::PeerRejected
                                    }
                                    CallRequestRejectReason::MutedByUser => {
                                        CallEndReason::PeerMuted
                                    }
                                    CallRequestRejectReason::Other => CallEndReason::Other,
                                };

                                let active_call = self.active_call().unwrap();
                                active_call.set_state(CallState::Ended(end_reason));
                            }
                            CallRequestResponse::Cancelled => unreachable!(),
                        }
                    }
                    PublishData::Alert(alert_type) => {
                        let peer = self
                            .peer_list()
                            .get(&their_peer_id)
                            .context("Received alert from unknown peer")?;

                        if !Application::get().settings().is_allowed_peer(&peer.name()) {
                            tracing::debug!("Ignored alert from muted peer");

                            return Ok(());
                        }

                        let peer = self.peer_list().get(&their_peer_id).unwrap();
                        self.emit_by_name::<()>("alert-received", &[&peer, &alert_type]);
                    }
                    other_published_data => {
                        tracing::debug!("Ignoring published data: {:?}", other_published_data);
                    }
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Subscribed {
                ..
            })) => {
                // We sometimes detect peer subscription to topic first before they discover and
                // add us to their peer list. So add a bit of delay before publishing our properties.
                glib::timeout_future(Duration::from_millis(200)).await;

                self.publish_properties().await;
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
enum CallRequestRejectReason {
    AlreadyInCall,
    RejectedByUser,
    MutedByUser,
    Other,
}

#[derive(Debug, Serialize, Deserialize)]
enum CallRequestResponse {
    Accept,
    Reject(CallRequestRejectReason),
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
enum CallIncomingResponse {
    Accept,
    Reject,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
enum Property {
    Name(String),
    Location(Option<Location>),
    Speed(f64),
    SignalQuality(SignalQuality),
    IconName(String),
}

#[derive(Debug, Serialize, Deserialize)]
enum PublishData {
    PropertyChanged(Vec<Property>),
    Alert(AlertType),
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

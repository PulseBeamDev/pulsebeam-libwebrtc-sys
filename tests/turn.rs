//! Fixture-backed production ICE tests. The Linux runtime image supplies coturn.
//! Signaling transfers complete gathered SDP, never individual candidates.

use std::{
    net::{IpAddr, TcpListener, ToSocketAddrs, UdpSocket},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use pulsebeam_webrtc_sys::{
    ConnectionState, IceGatheringState, IceServer, IceTransportPolicy, OperationId,
    PeerConfiguration, PeerConnection, PeerConnectionEvent, PeerConnectionFactory,
    SessionDescription,
};

struct TurnServer(Child);

impl TurnServer {
    fn stop(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }

    fn start() -> (Self, IpAddr, u16) {
        // Use a routable interface, not loopback: the default WebRTC network
        // manager does not offer loopback ICE interfaces in production mode.
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.connect("192.0.2.1:9").unwrap();
        let ip = socket.local_addr().unwrap().ip();
        let port = TcpListener::bind((ip, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let child = Command::new("turnserver")
            .args([
                "-n",
                "--no-cli",
                "--no-tls",
                "--no-dtls",
                "--no-multicast-peers",
                "--lt-cred-mech",
                "--realm=fixture.invalid",
                "--user=alice:secret",
                "--listening-ip",
                &ip.to_string(),
                "--relay-ip",
                &ip.to_string(),
                "--listening-port",
                &port.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("coturn is required by the Linux runtime test image");
        let mut server = Self(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            assert!(
                server.0.try_wait().unwrap().is_none(),
                "coturn exited early"
            );
            if std::net::TcpStream::connect_timeout(&(ip, port).into(), Duration::from_millis(20))
                .is_ok()
            {
                return (server, ip, port);
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("coturn did not listen on {ip}:{port}");
    }
}

impl Drop for TurnServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for<T>(
    peer: &PeerConnection,
    label: &str,
    events: &mut Vec<PeerConnectionEvent>,
    check: impl FnMut(&PeerConnectionEvent) -> Option<T>,
) -> T {
    wait_for_timeout(peer, label, events, Duration::from_secs(15), check)
}

fn wait_for_timeout<T>(
    peer: &PeerConnection,
    label: &str,
    events: &mut Vec<PeerConnectionEvent>,
    timeout: Duration,
    mut check: impl FnMut(&PeerConnectionEvent) -> Option<T>,
) -> T {
    let deadline = Instant::now() + timeout;
    if let Some(found) = events.iter().find_map(&mut check) {
        return found;
    }
    while Instant::now() < deadline {
        while let Some(event) = peer.try_next_event() {
            let found = check(&event);
            events.push(event);
            if let Some(found) = found {
                return found;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("timed out waiting for {label}: {events:#?}");
}

fn succeeded(
    peer: &PeerConnection,
    id: OperationId,
    events: &mut Vec<PeerConnectionEvent>,
) -> Option<SessionDescription> {
    wait_for(peer, "operation completion", events, |event| match event {
        PeerConnectionEvent::OperationComplete(result) if result.operation_id == id => {
            Some(result.result.clone().expect("operation failed"))
        }
        _ => None,
    })
}

fn gathered(peer: &PeerConnection, events: &mut Vec<PeerConnectionEvent>) -> SessionDescription {
    wait_for(peer, "ICE gathering completion", events, |event| {
        if matches!(
            event,
            PeerConnectionEvent::IceGatheringStateChanged(IceGatheringState::Complete)
        ) {
            Some(
                peer.descriptions()
                    .unwrap()
                    .pending_local
                    .or_else(|| peer.descriptions().unwrap().current_local)
                    .expect("missing local description"),
            )
        } else {
            None
        }
    })
}

#[test]
fn bad_turn_credentials_fail_without_relay_candidates() {
    let (_server, ip, port) = TurnServer::start();
    let factory = PeerConnectionFactory::builder().build().unwrap();
    let mut peer = factory
        .create_peer_connection(PeerConfiguration {
            ice_servers: vec![IceServer {
                urls: vec![format!("turn:{ip}:{port}?transport=udp")],
                username: "alice".into(),
                password: "invalid".into(),
            }],
            ice_transport_policy: IceTransportPolicy::RelayOnly,
            ..PeerConfiguration::default()
        })
        .unwrap();
    let mut events = Vec::new();
    let offer = succeeded(&peer, peer.create_offer(), &mut events).unwrap();
    succeeded(&peer, peer.set_local_description(offer), &mut events);
    let description = gathered(&peer, &mut events);
    assert!(!description.sdp.contains("typ relay"), "{description:?}");
    assert!(
        events.iter().any(|event| matches!(event, PeerConnectionEvent::IceCandidateError { url, .. } if url.contains("turn:"))),
        "TURN authentication failure must surface as an ICE candidate error: {events:#?}"
    );
    peer.close().unwrap();
}

#[test]
fn relay_only_udp_and_tcp_with_hostname_and_credentials() {
    let (mut server, ip, port) = TurnServer::start();
    let hostname = String::from_utf8(Command::new("hostname").output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_owned();
    assert!(
        (hostname.as_str(), port)
            .to_socket_addrs()
            .unwrap()
            .any(|address| address.ip() == ip),
        "fixture hostname must resolve to the listening interface"
    );
    for transport in ["udp", "tcp"] {
        let factory = PeerConnectionFactory::builder().build().unwrap();
        let ice_server = IceServer {
            urls: vec![format!("turn:{hostname}:{port}?transport={transport}")],
            username: "alice".into(),
            password: "secret".into(),
        };
        let config = PeerConfiguration {
            ice_servers: vec![ice_server],
            ice_transport_policy: IceTransportPolicy::RelayOnly,
            ..PeerConfiguration::default()
        };
        let mut alice = factory.create_peer_connection(config.clone()).unwrap();
        let mut bob = factory.create_peer_connection(config).unwrap();
        let mut alice_events = Vec::new();
        let mut bob_events = Vec::new();
        let offer = succeeded(&alice, alice.create_offer(), &mut alice_events).unwrap();
        succeeded(
            &alice,
            alice.set_local_description(offer),
            &mut alice_events,
        );
        let gathered_offer = gathered(&alice, &mut alice_events);
        assert!(
            gathered_offer.sdp.contains("typ relay"),
            "{transport}: {}",
            gathered_offer.sdp
        );
        assert!(gathered_offer.sdp.contains(&ip.to_string()));
        succeeded(
            &bob,
            bob.set_remote_description(gathered_offer),
            &mut bob_events,
        );
        let answer = succeeded(&bob, bob.create_answer(), &mut bob_events).unwrap();
        succeeded(&bob, bob.set_local_description(answer), &mut bob_events);
        let gathered_answer = gathered(&bob, &mut bob_events);
        assert!(
            gathered_answer.sdp.contains("typ relay"),
            "{transport}: {}",
            gathered_answer.sdp
        );
        succeeded(
            &alice,
            alice.set_remote_description(gathered_answer),
            &mut alice_events,
        );
        wait_for(&alice, "relay connection", &mut alice_events, |event| {
            matches!(
                event,
                PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected)
            )
            .then_some(())
        });
        if transport == "tcp" {
            // The only candidate pair uses this relay. Once it disappears,
            // the native peer must expose the broken connection to Rust.
            while let Some(event) = alice.try_next_event() {
                alice_events.push(event);
            }
            server.stop();
            wait_for_timeout(
                &alice,
                "connection loss after TURN shutdown",
                &mut Vec::new(),
                Duration::from_secs(45),
                |event| {
                    matches!(
                        event,
                        PeerConnectionEvent::ConnectionStateChanged(
                            ConnectionState::Disconnected | ConnectionState::Failed
                        )
                    )
                    .then_some(())
                },
            );
        }
        alice.close().unwrap();
        bob.close().unwrap();
    }
}

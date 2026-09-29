//! A real TLS server driven entirely by caller-delivered simulated TCP bytes.
use std::{
    fs,
    io::{Cursor, Read},
    net::Ipv4Addr,
    path::Path,
    process::Command,
    sync::Arc,
    time::Duration,
};

use pulsebeam_webrtc_sys::{
    ControlledPeerDriver, ControlledSimulatedNetwork, Environment, IceServer, ManualClock,
    OutboundKind, PeerConfiguration, PeerConnectionEvent, PeerConnectionFactory, TaskQueueFactory,
};
use rustls::{
    ServerConfig, ServerConnection,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};

fn openssl(args: &[&str]) {
    let output = Command::new("openssl").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn certificate(directory: &Path) -> (String, Vec<u8>, Vec<u8>) {
    let ca = directory.join("ca.pem");
    let ca_key = directory.join("ca.key");
    let cert = directory.join("server.pem");
    let key = directory.join("server.key");
    let request = directory.join("server.csr");
    let extensions = directory.join("server.ext");
    fs::write(&extensions, "subjectAltName=DNS:relay.test\nbasicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\nkeyUsage=digitalSignature,keyEncipherment\n").unwrap();
    openssl(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-sha256",
        "-days",
        "1",
        "-keyout",
        ca_key.to_str().unwrap(),
        "-out",
        ca.to_str().unwrap(),
        "-subj",
        "/CN=Controlled TURN CA",
        "-addext",
        "basicConstraints=critical,CA:TRUE",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
    ]);
    openssl(&[
        "req",
        "-new",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        key.to_str().unwrap(),
        "-out",
        request.to_str().unwrap(),
        "-subj",
        "/CN=relay.test",
    ]);
    openssl(&[
        "x509",
        "-req",
        "-in",
        request.to_str().unwrap(),
        "-CA",
        ca.to_str().unwrap(),
        "-CAkey",
        ca_key.to_str().unwrap(),
        "-CAcreateserial",
        "-out",
        cert.to_str().unwrap(),
        "-days",
        "1",
        "-sha256",
        "-extfile",
        extensions.to_str().unwrap(),
    ]);
    let cert_der = directory.join("server.der");
    let key_der = directory.join("server-key.der");
    openssl(&[
        "x509",
        "-in",
        cert.to_str().unwrap(),
        "-outform",
        "DER",
        "-out",
        cert_der.to_str().unwrap(),
    ]);
    openssl(&[
        "pkcs8",
        "-topk8",
        "-nocrypt",
        "-in",
        key.to_str().unwrap(),
        "-outform",
        "DER",
        "-out",
        key_der.to_str().unwrap(),
    ]);
    (
        fs::read_to_string(ca).unwrap(),
        fs::read(cert_der).unwrap(),
        fs::read(key_der).unwrap(),
    )
}

#[test]
fn turn_tls_exchanges_handshake_and_encrypted_turn_request_via_simulator() {
    let temp =
        std::env::temp_dir().join(format!("pulsebeam-controlled-tls-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let (ca_pem, cert_der, key_der) = certificate(&temp);
    let config =
        ServerConfig::builder_with_provider(Arc::new(oxitls_rustcrypto_provider::provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(cert_der)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
            )
            .unwrap();
    let mut server = ServerConnection::new(Arc::new(config)).unwrap();
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let queues = TaskQueueFactory::cooperative(&clock).unwrap();
    let environment = Environment::builder()
        .task_queue_factory(&queues)
        .build()
        .unwrap();
    let driver = ControlledPeerDriver::new(&clock).unwrap();
    let network = ControlledSimulatedNetwork::new(&clock, &driver).unwrap();
    let client_ip = Ipv4Addr::new(10, 71, 0, 1).into();
    let server_ip = Ipv4Addr::new(10, 71, 0, 2).into();
    let client = network.register_endpoint(client_ip).unwrap();
    let remote = network.register_endpoint(server_ip).unwrap();
    network.add_dns_record("relay.test", server_ip).unwrap();
    let factory = PeerConnectionFactory::builder()
        .environment(environment)
        .controlled_driver(&driver)
        .network_manager(client.network_manager().unwrap())
        .packet_socket_factory(client.packet_socket_factory().unwrap())
        .build()
        .unwrap();
    let mut peer = factory
        .create_peer_connection(PeerConfiguration {
            ice_servers: vec![IceServer {
                urls: vec!["turns:relay.test:5349?transport=tcp".into()],
                username: "client".into(),
                password: "secret".into(),
            }],
            turn_tls_ca_pem: Some(ca_pem),
            ..PeerConfiguration::default()
        })
        .unwrap();
    let operation = peer.create_offer();
    let mut offer = None;
    for _ in 0..1_000 {
        driver.run_ready();
        queues.run_ready();
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(result) = event {
                if result.operation_id == operation {
                    offer = Some(result.result.unwrap().unwrap());
                }
            }
        }
        if offer.is_some() {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
    }
    peer.set_local_description(offer.expect("offer not created"));

    let mut connected = false;
    let mut request = false;
    for _ in 0..10_000 {
        driver.run_ready();
        queues.run_ready();
        while let Some(packet) = network.next_packet() {
            match packet.kind {
                OutboundKind::TcpConnect => {
                    assert_eq!(packet.destination.ip(), server_ip);
                    assert_eq!(packet.destination.port(), 5349);
                    connected = true;
                    network.deliver(packet.id).unwrap();
                }
                OutboundKind::TcpData => {
                    assert!(connected);
                    server.read_tls(&mut Cursor::new(&packet.payload)).unwrap();
                    server.process_new_packets().unwrap();
                    let mut decrypted = [0u8; 1024];
                    match server.reader().read(&mut decrypted) {
                        Ok(size) if size > 0 => {
                            assert_eq!(decrypted[0] & 0b1100_0000, 0, "not a STUN request");
                            request = true;
                        }
                        Ok(_) | Err(_) => {}
                    }
                    network.drop_packet(packet.id).unwrap();
                    let mut response = Vec::new();
                    while server.wants_write() {
                        server.write_tls(&mut response).unwrap();
                    }
                    if !response.is_empty() {
                        network
                            .inject_tcp_data(packet.destination, packet.source, &response)
                            .unwrap();
                    }
                }
                OutboundKind::Udp => network.drop_packet(packet.id).unwrap(),
            }
        }
        if request {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
    }
    assert!(connected, "no simulated TCP connection");
    assert!(
        request,
        "no decrypted TURN request after verified TLS handshake"
    );
    peer.close().unwrap();
    drop(peer);
    drop(factory);
    drop(remote);
    drop(client);
    drop(network);
    drop(driver);
    fs::remove_dir_all(temp).unwrap();
}

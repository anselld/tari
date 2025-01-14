//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

// NOTE: This test uses ports 11111 and 11112

use crate::support::{assert_change, random_string};
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsServices,
    connection::NetAddress,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer},
    types::CommsDatabase,
    CommsBuilder,
};
use tari_p2p::{
    ping_pong::{PingPongService, PingPongServiceApi},
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::TariMessageType,
};
use tari_storage::lmdb_store::LMDBBuilder;
use tempdir::TempDir;

fn new_node_identity(control_service_address: NetAddress) -> NodeIdentity {
    NodeIdentity::random(&mut OsRng::new().unwrap(), control_service_address).unwrap()
}

fn create_peer_storage(tmpdir: &TempDir, database_name: &str, peers: Vec<Peer>) -> CommsDatabase {
    let datastore = LMDBBuilder::new()
        .set_path(tmpdir.path().to_str().unwrap())
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(database_name).unwrap();
    let mut storage = PeerStorage::new(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into_datastore()
}

fn setup_ping_pong_service(
    node_identity: NodeIdentity,
    peer_storage: CommsDatabase,
) -> (
    ServiceExecutor,
    Arc<PingPongServiceApi>,
    Arc<CommsServices<TariMessageType>>,
)
{
    let ping_pong = PingPongService::new();
    let pingpong_api = ping_pong.get_api();

    let services = ServiceRegistry::new().register(ping_pong);
    let comms = CommsBuilder::new()
        .with_routes(services.build_comms_routes())
        .with_node_identity(node_identity.clone())
        .with_peer_storage(peer_storage)
        .configure_peer_connections(PeerConnectionConfig {
            host: "127.0.0.1".parse().unwrap(),
            ..Default::default()
        })
        .configure_control_service(ControlServiceConfig {
            socks_proxy_address: None,
            listener_address: node_identity.control_service_address.clone(),
            requested_connection_timeout: Duration::from_millis(5000),
        })
        .build()
        .unwrap()
        .start()
        .map(Arc::new)
        .unwrap();

    (ServiceExecutor::execute(&comms, services), pingpong_api, comms)
}

#[test]
#[allow(non_snake_case)]
fn end_to_end() {
    let node_A_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();

    let node_B_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();

    let node_A_identity = new_node_identity("127.0.0.1:11111".parse().unwrap());
    let node_B_identity = new_node_identity("127.0.0.1:11112".parse().unwrap());

    let (node_A_services, node_A_pingpong, _comms_A) = setup_ping_pong_service(
        node_A_identity.clone(),
        create_peer_storage(&node_A_tmpdir, "node_A", vec![node_B_identity.clone().into()]),
    );

    let (node_B_services, node_B_pingpong, _comms_B) = setup_ping_pong_service(
        node_B_identity.clone(),
        create_peer_storage(&node_B_tmpdir, "node_B", vec![node_A_identity.clone().into()]),
    );

    // Ping node B
    node_A_pingpong
        .ping(node_B_identity.identity.public_key.clone())
        .unwrap();

    assert_change(|| node_B_pingpong.ping_count().unwrap(), 1, 20);
    assert_change(|| node_A_pingpong.pong_count().unwrap(), 1, 20);

    // Ping node A
    node_B_pingpong
        .ping(node_A_identity.identity.public_key.clone())
        .unwrap();

    assert_change(|| node_B_pingpong.pong_count().unwrap(), 1, 20);
    assert_change(|| node_A_pingpong.ping_count().unwrap(), 1, 20);

    node_A_services.shutdown().unwrap();
    node_B_services.shutdown().unwrap();
}

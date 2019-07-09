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

use super::{
    connections::LivePeerConnections,
    establisher::ConnectionEstablisher,
    protocol::PeerConnectionProtocol,
    ConnectionManagerError,
    PeerConnectionConfig,
    Result,
};
use crate::{
    connection::{ConnectionError, CurvePublicKey, NetAddress, PeerConnection, PeerConnectionState, ZmqContext},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerManager},
};
use log::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_utilities::thread_join::thread_join::ThreadJoinWithTimeout;

const LOG_TARGET: &'static str = "comms::connection_manager::manager";

pub struct ConnectionManager {
    node_identity: Arc<NodeIdentity>,
    connections: LivePeerConnections,
    establisher: Arc<ConnectionEstablisher>,
    peer_manager: Arc<PeerManager>,
    establish_locks: Mutex<HashMap<NodeId, Arc<Mutex<()>>>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(
        zmq_context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        config: PeerConnectionConfig,
    ) -> Self
    {
        Self {
            node_identity,
            connections: LivePeerConnections::with_max_connections(config.max_connections),
            establisher: Arc::new(ConnectionEstablisher::new(zmq_context, config, peer_manager.clone())),
            peer_manager,
            establish_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Attempt to establish a connection to a given peer. If the connection exists
    /// the existing connection is returned.
    pub fn establish_connection_to_peer(&self, peer: &Peer) -> Result<Arc<PeerConnection>> {
        self.with_establish_lock(&peer.node_id, || self.attempt_peer_connection(peer))
    }

    /// Attempt to establish a connection to a given NodeId. If the connection exists
    /// the existing connection is returned.
    pub fn establish_connection_to_node_id(&self, node_id: &NodeId) -> Result<Arc<PeerConnection>> {
        match self.peer_manager.find_with_node_id(node_id) {
            Ok(peer) => self.with_establish_lock(node_id, || self.attempt_peer_connection(&peer)),
            Err(err) => Err(ConnectionManagerError::PeerManagerError(err)),
        }
    }

    /// Establish an outbound connection for the given peer to the given address using the given
    /// CurvePublicKey.
    ///
    /// ## Arguments
    ///
    /// `peer`: &Peer - The peer which issued the request
    /// `address`: NetAddress - The address of the destination connection
    /// `dest_public_key`: &Peer - The Curve25519 public key of the destination connection
    pub(crate) fn establish_requested_outbound_connection(
        &self,
        peer: &Peer,
        address: NetAddress,
        dest_public_key: CurvePublicKey,
    ) -> Result<Arc<PeerConnection>>
    {
        // If we have reached the maximum connections, we won't allow new connections to be requested
        if self.connections.has_reached_max_active_connections() {
            return Err(ConnectionManagerError::MaxConnectionsReached);
        }

        let (conn, join_handle) = self.establisher.establish_outbound_peer_connection(
            peer.node_id.clone().into(),
            address,
            dest_public_key,
        )?;

        self.connections
            .add_connection(peer.node_id.clone(), conn.clone(), join_handle)?;
        Ok(conn)
    }

    pub fn shutdown(self) -> Vec<std::result::Result<(), ConnectionError>> {
        self.connections.shutdown_joined()
    }

    /// Lock a critical section for the given node id during connection establishment
    fn with_establish_lock<T>(&self, node_id: &NodeId, func: impl Fn() -> T) -> T {
        // Return the lock for the given node id. If no lock exists create a new one and return it.
        let nid_lock = {
            let mut establish_locks = acquire_lock!(self.establish_locks);
            match establish_locks.get(node_id) {
                Some(lock) => lock.clone(),
                None => {
                    let new_lock = Arc::new(Mutex::new(()));
                    establish_locks.insert(node_id.clone(), new_lock.clone());
                    new_lock
                },
            }
        };

        // Lock the lock for the NodeId
        let _nid_lock_guard = acquire_lock!(nid_lock);
        let ret = func();
        // Remove establish lock once done to release memory. This is safe because the function has already
        // established the connection, so any subsequent calls will return the existing connection.
        {
            let mut establish_locks = acquire_lock!(self.establish_locks);
            establish_locks.remove(node_id);
        }
        ret
    }

    fn attempt_peer_connection(&self, peer: &Peer) -> Result<Arc<PeerConnection>> {
        let maybe_conn = self.connections.get_connection(&peer.node_id);
        let peer_conn = match maybe_conn {
            Some(conn) => {
                let state = conn.get_state();

                match state {
                    PeerConnectionState::Initial |
                    PeerConnectionState::Disconnected |
                    PeerConnectionState::Shutdown => {
                        warn!(
                            target: LOG_TARGET,
                            "Peer connection state is '{}'. Attempting to reestablish connection to peer.", state
                        );
                        // Ignore not found error when dropping
                        let _ = self.connections.drop_connection(&peer.node_id);
                        self.initiate_peer_connection(peer)?
                    },
                    PeerConnectionState::Failed(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Peer connection for NodeId={} in failed state. Error({:?}) Attempting to reestablish.",
                            peer.node_id,
                            err
                        );
                        // Ignore not found error when dropping
                        self.connections.drop_connection(&peer.node_id)?;
                        self.initiate_peer_connection(peer)?
                    },
                    // Already have an active connection, just return it
                    PeerConnectionState::Listening(Some(address)) => {
                        debug!(
                            target: LOG_TARGET,
                            "Waiting for NodeId={} to connect at {}...", peer.node_id, address
                        );
                        return Ok(conn);
                    },
                    PeerConnectionState::Listening(None) => {
                        debug!(
                            target: LOG_TARGET,
                            "Listening on non-tcp socket for NodeId={}...", peer.node_id
                        );
                        return Ok(conn);
                    },
                    PeerConnectionState::Connecting => {
                        debug!(target: LOG_TARGET, "Still connecting to {}...", peer.node_id);
                        return Ok(conn);
                    },
                    PeerConnectionState::Connected(Some(address)) => {
                        debug!("Connection already established to {}.", address);
                        return Ok(conn);
                    },
                    PeerConnectionState::Connected(None) => {
                        debug!("Connection already established to non-TCP socket");
                        return Ok(conn);
                    },
                }
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Peer connection does not exist for NodeId={}", peer.node_id
                );
                self.initiate_peer_connection(peer)?
            },
        };

        Ok(peer_conn.clone())
    }

    /// Get the peer manager
    pub(crate) fn get_peer_manager(&self) -> Arc<PeerManager> {
        self.peer_manager.clone()
    }

    /// Shutdown a given peer's [PeerConnection] and return it if one exists,
    /// otherwise None is returned.
    ///
    /// [PeerConnection]: ../../connection/peer_connection/index.html
    pub(crate) fn shutdown_connection_for_peer(&self, peer: &Peer) -> Result<Option<Arc<PeerConnection>>> {
        match self.connections.drop_connection(&peer.node_id) {
            Ok((conn, handle)) => {
                handle
                    .timeout_join(Duration::from_millis(3000))
                    .map_err(ConnectionManagerError::PeerConnectionThreadError)?;
                Ok(Some(conn))
            },
            Err(ConnectionManagerError::PeerConnectionNotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Return the number of _active_ peer connections currently managed by this instance
    pub fn get_active_connection_count(&self) -> usize {
        self.connections.get_active_connection_count()
    }

    fn initiate_peer_connection(&self, peer: &Peer) -> Result<Arc<PeerConnection>> {
        let protocol = PeerConnectionProtocol::new(&self.node_identity, &self.establisher);
        self.peer_manager
            .reset_connection_attempts(&peer.node_id)
            .map_err(ConnectionManagerError::PeerManagerError)?;

        protocol
            .negotiate_peer_connection(peer)
            .and_then(|(new_inbound_conn, join_handle)| {
                debug!(
                    target: LOG_TARGET,
                    "[{:?}] Waiting for peer connection acceptance from remote peer ",
                    new_inbound_conn.get_address()
                );
                let config = self.establisher.get_config();
                // Wait for a message from the peer before continuing
                new_inbound_conn
                    .wait_connected_or_failure(&config.peer_connection_establish_timeout)
                    .or_else(|err| {
                        info!(
                            target: LOG_TARGET,
                            "Peer did not accept the connection within {:?} [NodeId={}] : {:?}",
                            config.peer_connection_establish_timeout,
                            peer.node_id,
                            err,
                        );
                        Err(ConnectionManagerError::ConnectionError(err))
                    })?;

                self.connections
                    .add_connection(peer.node_id.clone(), Arc::clone(&new_inbound_conn), join_handle)?;

                Ok(new_inbound_conn)
            })
            .or_else(|err| {
                warn!(
                    target: LOG_TARGET,
                    "Failed to establish peer connection to NodeId={}", peer.node_id
                );
                warn!(
                    target: LOG_TARGET,
                    "Failed connection error for NodeId={}: {:?}", peer.node_id, err
                );
                Err(err)
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::{InprocAddress, ZmqContext},
        peer_manager::PeerFlags,
        types::CommsPublicKey,
    };
    use rand::rngs::OsRng;
    use std::{path::PathBuf, thread, time::Duration};
    use tari_crypto::keys::PublicKey;
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBStore};

    fn get_path(name: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name);
        path.to_str().unwrap().to_string()
    }

    fn init_datastore(name: &str) -> Result<LMDBStore> {
        let path = get_path(name);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(10)
            .set_max_number_of_databases(2)
            .add_database(name, lmdb_zero::db::CREATE)
            .build()
            .map_err(|_| ConnectionManagerError::DatastoreError)
    }

    fn clean_up_datastore(name: &str) {
        std::fs::remove_dir_all(get_path(name)).unwrap();
    }

    fn setup(database_name: &str) -> (ZmqContext, Arc<NodeIdentity>, Arc<PeerManager>) {
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();
        let peer_manager = Arc::new(PeerManager::new(peer_database).unwrap());

        (context, node_identity, peer_manager)
    }

    fn create_peer(address: NetAddress) -> Peer {
        let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng::new().unwrap());
        let node_id = NodeId::from_key(&pk).unwrap();
        Peer::new(pk, node_id, address.into(), PeerFlags::empty())
    }

    #[test]
    fn get_active_connection_count() {
        let database_name = "connection_manager_get_active_connection_count";
        let (context, node_identity, peer_manager) = setup(database_name);
        let manager = ConnectionManager::new(context, node_identity, peer_manager, PeerConnectionConfig {
            peer_connection_establish_timeout: Duration::from_secs(5),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 3,
            max_connections: 10,
            message_sink_address: InprocAddress::random(),
            socks_proxy_address: None,
        });

        assert_eq!(manager.get_active_connection_count(), 0);

        clean_up_datastore(database_name);
    }

    #[test]
    fn shutdown_connection_for_peer() {
        let database_name = "connection_manager_shutdown_connection_for_peer";
        let (context, node_identity, peer_manager) = setup(database_name);
        let manager = ConnectionManager::new(context, node_identity, peer_manager, PeerConnectionConfig {
            peer_connection_establish_timeout: Duration::from_secs(5),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 3,
            max_connections: 10,
            message_sink_address: InprocAddress::random(),
            socks_proxy_address: None,
        });

        assert_eq!(manager.get_active_connection_count(), 0);

        let address = "127.0.0.1:43456".parse::<NetAddress>().unwrap();
        let peer = create_peer(address.clone());

        assert!(manager.shutdown_connection_for_peer(&peer).unwrap().is_none());

        let (peer_conn, rx) = PeerConnection::active_state_for_test();
        let peer_conn = Arc::new(peer_conn);
        let join_handle = thread::spawn(|| Ok(()));
        manager
            .connections
            .add_connection(peer.node_id.clone(), peer_conn, join_handle)
            .unwrap();

        match manager.shutdown_connection_for_peer(&peer).unwrap() {
            Some(_) => {},
            None => panic!("shutdown_connection_for_peer did not return active peer connection"),
        }

        drop(rx);

        clean_up_datastore(database_name);
    }
}

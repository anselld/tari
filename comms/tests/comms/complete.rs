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

use std::{collections::HashMap, sync::Arc, time::Duration};
use tari_comms::{
    connection::{types::SocketType, zmq::*, *},
    connection_manager::*,
    control_service::{handlers as comms_handlers, *},
    dispatcher::*,
    outbound_message_service::outbound_message_service::*,
    peer_manager::*,
    types::*,
};
use tari_storage::lmdb::LMDBStore;
#[test]

fn establish_connection() {
    // create node 1
    let context = Context::new();
    let listener_address1 = "0.0.0.0:7899".parse::<NetAddress>().unwrap();

    let conn_manager1 = Arc::new(ConnectionManager::new(&context, PeerConnectionConfig {
        max_message_size: 1024,
        max_connect_retries: 1,
        socks_proxy_address: None,
        consumer_address: InprocAddress::random(),
        port_range: 10000..11000,
        host: "0.0.0.0".parse().unwrap(),
        establish_timeout: Duration::from_millis(1000),
    }));

    let peer_manager1 = Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap());

    let dispatcher1 = Dispatcher::new(comms_handlers::ControlServiceResolver {})
        .route(
            ControlServiceMessageType::EstablishConnection,
            comms_handlers::establish_connection,
        )
        .route(ControlServiceMessageType::Accept, self::accept_local)
        .catch_all(comms_handlers::discard);

    let service1 = ControlService::new(&context)
        .configure(ControlServiceConfig {
            listener_address: listener_address1.clone(),
            socks_proxy_address: None,
        })
        .serve(dispatcher1, conn_manager1, peer_manager1.clone())
        .unwrap();

    let Socket1 = context
        .socket(SocketType::Reply)
        .map_err(|e| OutboundError::SocketError(e))
        .unwrap();
    Socket1
        .bind(&listener_address1.to_zmq_endpoint())
        .map_err(|e| OutboundError::SocketConnectionError(e))
        .unwrap();

    // create node 2
    let context = Context::new();
    let listener_address2 = "0.0.0.0:7999".parse::<NetAddress>().unwrap();

    let conn_manager2 = Arc::new(ConnectionManager::new(&context, PeerConnectionConfig {
        max_message_size: 1024,
        max_connect_retries: 1,
        socks_proxy_address: None,
        consumer_address: InprocAddress::random(),
        port_range: 10000..11000,
        host: "0.0.0.0".parse().unwrap(),
        establish_timeout: Duration::from_millis(1000),
    }));

    let peer_manager2 = Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap());

    let dispatcher2 = Dispatcher::new(comms_handlers::ControlServiceResolver {})
        .route(
            ControlServiceMessageType::EstablishConnection,
            comms_handlers::establish_connection,
        )
        .route(ControlServiceMessageType::Accept, self::accept_local)
        .catch_all(comms_handlers::discard);

    let service2 = ControlService::new(&context)
        .configure(ControlServiceConfig {
            listener_address: listener_address2,
            socks_proxy_address: None,
        })
        .serve(dispatcher2, conn_manager2, peer_manager2.clone())
        .unwrap();

    assert!(service1.shutdown().is_ok());
    assert!(service2.shutdown().is_ok());
}

/// The peer has accepted the request to connect
pub fn accept_local(context: ControlServiceMessageContext) -> Result<(), ControlServiceError> {
    dbg!(1);
    Ok(())
}

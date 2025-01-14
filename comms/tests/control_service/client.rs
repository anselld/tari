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

use crate::support::factories::{self, TestFactory};
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::{Connection, Direction, InprocAddress, ZmqContext},
    control_service::{messages::Ping, ControlServiceClient},
};

#[test]
fn send_ping_recv_pong() {
    let context = ZmqContext::new();
    let address = InprocAddress::random();

    let outbound_conn = Connection::new(&context, Direction::Outbound)
        .establish(&address)
        .unwrap();
    let inbound_conn = Connection::new(&context, Direction::Inbound)
        .establish(&address)
        .unwrap();

    let node_identity_1 = factories::node_identity::create().build().map(Arc::new).unwrap();
    let node_identity_2 = factories::node_identity::create().build().map(Arc::new).unwrap();

    let out_client = ControlServiceClient::new(
        node_identity_1.clone(),
        node_identity_2.identity.public_key.clone(),
        outbound_conn,
    );
    out_client.send_ping().unwrap();

    let in_client = ControlServiceClient::new(
        node_identity_2.clone(),
        node_identity_1.identity.public_key.clone(),
        inbound_conn,
    );

    let _msg: Ping = in_client.receive_message(Duration::from_millis(2000)).unwrap().unwrap();
}

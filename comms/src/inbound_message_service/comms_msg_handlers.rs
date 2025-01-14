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

use crate::{
    consts::DHT_FORWARD_NODE_COUNT,
    dispatcher::{DispatchError, DispatchResolver, DispatchableKey},
    message::{DomainMessageContext, Message, MessageContext, MessageFlags, MessageHeader, NodeDestination},
    outbound_message_service::BroadcastStrategy,
    types::MessageDispatcher,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use tari_utilities::message_format::MessageFormat;

const LOG_TARGET: &str = "comms::inbound_message_service::handlers";

/// The comms_msg_dispatcher will determine the type of message and forward it to the the correct handler
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub enum CommsDispatchType {
    // Messages of this type must be handled
    Handle,
    // Messages of this type must be forwarded to peers
    Forward,
    // Messages of this type can be ignored and discarded
    Discard,
}

/// Specify what handler function should be called for messages with different comms level dispatch types
pub fn construct_comms_msg_dispatcher<MType>() -> MessageDispatcher<MessageContext<MType>>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
    MType: Debug,
{
    MessageDispatcher::new(InboundMessageServiceResolver {})
        .route(CommsDispatchType::Handle, handler_handle)
        .route(CommsDispatchType::Forward, handler_forward)
        .route(CommsDispatchType::Discard, handler_discard)
}

#[derive(Clone)]
pub struct InboundMessageServiceResolver;

impl<MType> DispatchResolver<CommsDispatchType, MessageContext<MType>> for InboundMessageServiceResolver
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    /// The dispatch type is determined from the content of the MessageContext, which is used to dispatch the message to
    /// the correct handler
    fn resolve(&self, message_context: &MessageContext<MType>) -> Result<CommsDispatchType, DispatchError> {
        let message_envelope = &message_context.message_envelope;

        // Check destination of message
        let message_envelope_header = message_envelope
            .deserialize_header()
            .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;

        // Verify source node message signature
        if !message_envelope_header
            .verify_signatures(message_envelope.body_frame().clone())
            .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?
        {
            return Ok(CommsDispatchType::Discard);
        }

        let node_identity = &message_context.node_identity;
        let peer_manager = &message_context.peer_manager;

        match message_envelope_header.dest {
            NodeDestination::Unknown => Ok(CommsDispatchType::Handle),
            NodeDestination::PublicKey(dest_public_key) => {
                if node_identity.identity.public_key == dest_public_key {
                    Ok(CommsDispatchType::Handle)
                } else {
                    Ok(CommsDispatchType::Forward)
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                if peer_manager
                    .in_network_region(&dest_node_id, &node_identity.identity.node_id, DHT_FORWARD_NODE_COUNT)
                    .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?
                {
                    Ok(CommsDispatchType::Handle)
                } else {
                    Ok(CommsDispatchType::Forward)
                }
            },
        }
    }
}

fn handler_handle<MType>(message_context: MessageContext<MType>) -> Result<(), DispatchError>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
    MType: Debug,
{
    let envelope = &message_context.message_envelope;
    debug!(
        target: LOG_TARGET,
        "Received message, wire format version {:x?}",
        envelope.version_frame()
    );

    // Check encryption and retrieved Message
    let message_envelope_header = envelope
        .deserialize_header()
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;

    debug!(
        target: LOG_TARGET,
        "Handling message with origin signature {:x?}", message_envelope_header.origin_signature
    );
    let node_identity = &message_context.node_identity;
    let message: Message;
    if message_envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
        debug!(target: LOG_TARGET, "Attempting to decrypt message");
        match message_context
            .message_envelope
            .deserialize_encrypted_body(&node_identity.secret_key, &message_envelope_header.origin_source)
        {
            Ok(decrypted_message_body) => {
                debug!(target: LOG_TARGET, "Message successfully decrypted");
                message = decrypted_message_body;
            },
            Err(_) => {
                if message_envelope_header.dest == NodeDestination::Unknown {
                    debug!(
                        target: LOG_TARGET,
                        "Unable to decrypt message with unknown recipient, forwarding..."
                    );
                    // Message might have been for this node if it could have been decrypted
                    return handler_forward(message_context);
                } else {
                    warn!(target: LOG_TARGET, "Unable to decrypt message addressed to this node");
                    // Message was for this node but could not be decrypted
                    return handler_discard(message_context);
                }
            },
        }
    } else {
        debug!(target: LOG_TARGET, "Message not encrypted");
        message = message_context
            .message_envelope
            .deserialize_body()
            .map_err(DispatchError::handler_error())?;
    };

    // Construct DomainMessageContext and dispatch to handler services using domain message broker
    let header: MessageHeader<MType> = message.deserialize_header().unwrap(); //.map_err(DispatchError::handler_error())?;

    debug!(target: LOG_TARGET, "Received message type: {:?}", header.message_type);
    let domain_message_context = DomainMessageContext::new(
        message_context.peer.into(),
        message_envelope_header.origin_source,
        message,
        envelope.clone(),
    );
    let domain_message_context_buffer = vec![domain_message_context
        .to_binary()
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?];

    debug!(
        target: LOG_TARGET,
        "Dispatching message type: {:?}", header.message_type
    );
    message_context
        .inbound_message_broker
        .dispatch(header.message_type, &domain_message_context_buffer)
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))
}

fn handler_forward<MType>(message_context: MessageContext<MType>) -> Result<(), DispatchError>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    // Forward message using appropriate broadcast strategy based on the destination provided in the header
    let envelope = message_context.message_envelope;
    let message_envelope_header = envelope
        .deserialize_header()
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;
    let broadcast_strategy = BroadcastStrategy::forward(
        message_context.node_identity.identity.node_id.clone(),
        &message_context.peer_manager,
        message_envelope_header.dest,
        vec![
            message_envelope_header.origin_source,
            message_envelope_header.peer_source,
        ],
    )
    .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;

    debug!(target: LOG_TARGET, "Forwarding message");
    message_context
        .outbound_message_service
        .forward_message(broadcast_strategy, envelope)
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))
}

fn handler_discard<MType>(_message_context: MessageContext<MType>) -> Result<(), DispatchError>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    // TODO: Add logic for discarding a message

    Ok(())
}

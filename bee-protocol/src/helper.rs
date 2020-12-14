// Copyright 2020 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{
    milestone::MilestoneIndex,
    packet::{tlv_into_bytes, Heartbeat, Message as MessagePacket, MessageRequest, MilestoneRequest, Packet},
    peer::PeerManager,
    tangle::MsTangle,
    worker::{MessageRequesterWorkerEvent, MilestoneRequesterWorkerEvent, RequestedMessages, RequestedMilestones},
    ProtocolMetrics,
};

use bee_common::node::ResHandle;
use bee_message::MessageId;
use bee_network::{Command::SendMessage, NetworkController, PeerId};
use bee_storage::storage::Backend;

use log::warn;
use tokio::sync::mpsc;

use std::marker::PhantomData;

pub(crate) struct Sender<P: Packet> {
    marker: PhantomData<P>,
}

macro_rules! implement_sender_worker {
    ($type:ty, $sender:tt, $incrementor:tt) => {
        impl Sender<$type> {
            pub(crate) fn send(
                network: &NetworkController,
                metrics: &ResHandle<ProtocolMetrics>,
                id: &PeerId,
                packet: $type,
            ) {
                match network.send(SendMessage {
                    to: id.clone(),
                    message: tlv_into_bytes(packet),
                }) {
                    Ok(_) => {
                        // self.peer.metrics.$incrementor();
                        metrics.$incrementor();
                    }
                    Err(e) => {
                        warn!("Sending {} to {} failed: {:?}.", stringify!($type), id, e);
                    }
                }
            }
        }
    };
}

implement_sender_worker!(MilestoneRequest, milestone_request, milestone_requests_sent_inc);
implement_sender_worker!(MessagePacket, message, messages_sent_inc);
implement_sender_worker!(MessageRequest, message_request, message_requests_sent_inc);
implement_sender_worker!(Heartbeat, heartbeat, heartbeats_sent_inc);

// TODO move some functions to workers

// MilestoneRequest

pub(crate) fn request_milestone<B: Backend>(
    tangle: &MsTangle<B>,
    milestone_requester: &mpsc::UnboundedSender<MilestoneRequesterWorkerEvent>,
    requested_milestones: &RequestedMilestones,
    index: MilestoneIndex,
    to: Option<PeerId>,
) {
    if !requested_milestones.contains_key(&index) && !tangle.contains_milestone(index) {
        if let Err(e) = milestone_requester.send(MilestoneRequesterWorkerEvent(index, to)) {
            warn!("Requesting milestone failed: {}.", e);
        }
    }
}

pub(crate) fn request_latest_milestone<B: Backend>(
    tangle: &MsTangle<B>,
    milestone_requester: &mpsc::UnboundedSender<MilestoneRequesterWorkerEvent>,
    requested_milestones: &RequestedMilestones,
    to: Option<PeerId>,
) {
    request_milestone(tangle, milestone_requester, requested_milestones, MilestoneIndex(0), to)
}

// MessageRequest

pub(crate) async fn request_message<B: Backend>(
    tangle: &MsTangle<B>,
    message_requester: &mpsc::UnboundedSender<MessageRequesterWorkerEvent>,
    requested_messages: &RequestedMessages,
    message_id: MessageId,
    index: MilestoneIndex,
) {
    if !tangle.contains(&message_id).await
        && !tangle.is_solid_entry_point(&message_id)
        && !requested_messages.contains_key(&message_id)
    {
        if let Err(e) = message_requester.send(MessageRequesterWorkerEvent(message_id, index)) {
            warn!("Requesting message failed: {}.", e);
        }
    }
}

// Heartbeat

pub fn send_heartbeat(
    peer_manager: &ResHandle<PeerManager>,
    network: &NetworkController,
    metrics: &ResHandle<ProtocolMetrics>,
    to: PeerId,
    latest_solid_milestone_index: MilestoneIndex,
    pruning_milestone_index: MilestoneIndex,
    latest_milestone_index: MilestoneIndex,
) {
    Sender::<Heartbeat>::send(
        network,
        metrics,
        &to,
        Heartbeat::new(
            *latest_solid_milestone_index,
            *pruning_milestone_index,
            *latest_milestone_index,
            peer_manager.connected_peers(),
            peer_manager.synced_peers(),
        ),
    );
}

pub fn broadcast_heartbeat(
    peer_manager: &ResHandle<PeerManager>,
    network: &NetworkController,
    metrics: &ResHandle<ProtocolMetrics>,
    latest_solid_milestone_index: MilestoneIndex,
    pruning_milestone_index: MilestoneIndex,
    latest_milestone_index: MilestoneIndex,
) {
    for entry in peer_manager.peers.iter() {
        send_heartbeat(
            peer_manager,
            network,
            metrics,
            entry.key().clone(),
            latest_solid_milestone_index,
            pruning_milestone_index,
            latest_milestone_index,
        );
    }
}
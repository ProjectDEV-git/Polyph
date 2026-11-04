//! Messages exchanged inside the PAKE channel while joining.

use serde::{Deserialize, Serialize};

use crate::cert::{CertChain, Role};
use crate::delivery::EpochKeyDelivery;
use crate::identity::DevicePublic;
use crate::ids::{MemberId, RoomId};
use crate::signed::Signed;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRequest {
    pub device: DevicePublic,
    pub member: MemberId,
    pub display_name: String,
    #[serde(with = "crate::bytes::vec")]
    pub name_salt: Vec<u8>,
    pub surface: String,
    /// Set when linking another device of the same account rather than joining a room.
    #[serde(default)]
    pub link_device: bool,
    /// Device capabilities asked for (see `messages::AGENT_HOST`).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum JoinReply {
    /// The request is waiting for an Owner or Maintainer to approve the knock.
    Pending,
    Approved(Box<JoinApproved>),
    Denied {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinApproved {
    pub room: RoomId,
    pub room_name: Option<String>,
    pub role: Role,
    pub chain: CertChain,
    pub key: Signed<EpochKeyDelivery>,
    /// The relay URL the room lives on, for joins that came through a LAN relay.
    pub relay: Option<String>,
}

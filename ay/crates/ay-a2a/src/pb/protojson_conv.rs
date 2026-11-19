// SPDX-License-Identifier: Apache-2.0
// Copyright AGNTCY Contributors (https://github.com/agntcy)
use std::fmt;

use crate::{
    AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse, StreamResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig,
};
use prost::Message;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Debug)]
pub enum ProtoJsonPayloadError {
    Json(serde_json::Error),
    Decode(prost::DecodeError),
    MissingPayload(&'static str),
}

impl fmt::Display for ProtoJsonPayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "ProtoJSON error: {error}"),
            Self::Decode(error) => write!(f, "protobuf transcode error: {error}"),
            Self::MissingPayload(type_name) => {
                write!(f, "ProtoJSON payload missing required data for {type_name}")
            }
        }
    }
}

impl std::error::Error for ProtoJsonPayloadError {}

pub trait ProtoJsonPayload: Sized {
    type Proto: Message + Default;
    type ProtoJson: Message + Default + Serialize + DeserializeOwned;

    fn to_proto(value: &Self) -> Self::Proto;
    fn try_from_proto(value: &Self::Proto) -> Result<Self, ProtoJsonPayloadError>;
}

pub fn to_value<T: ProtoJsonPayload>(value: &T) -> Result<Value, ProtoJsonPayloadError> {
    let proto = T::to_proto(value);
    let protojson: T::ProtoJson = transcode_message(&proto)?;
    serde_json::to_value(protojson).map_err(ProtoJsonPayloadError::Json)
}

pub fn from_value<T: ProtoJsonPayload>(value: Value) -> Result<T, ProtoJsonPayloadError> {
    let protojson: T::ProtoJson =
        serde_json::from_value(value).map_err(ProtoJsonPayloadError::Json)?;
    let proto: T::Proto = transcode_message(&protojson)?;
    T::try_from_proto(&proto)
}

pub fn from_str<T: ProtoJsonPayload>(value: &str) -> Result<T, ProtoJsonPayloadError> {
    let protojson: T::ProtoJson =
        serde_json::from_str(value).map_err(ProtoJsonPayloadError::Json)?;
    let proto: T::Proto = transcode_message(&protojson)?;
    T::try_from_proto(&proto)
}

fn transcode_message<Src, Dst>(source: &Src) -> Result<Dst, ProtoJsonPayloadError>
where
    Src: Message,
    Dst: Message + Default,
{
    Dst::decode(source.encode_to_vec().as_slice()).map_err(ProtoJsonPayloadError::Decode)
}

// ---------------------------------------------------------------------------
// Native targets: `Proto` is the generated gRPC prost type from `src/gen/`
// (which has tonic stubs), `ProtoJson` is the OUT_DIR pbjson type.
// The two-type system lets `pbconv` convert between hand-written `a2a` types
// and the prost-generated types, then `transcode_message` re-encodes between
// the two generated types (identical wire format).
// ---------------------------------------------------------------------------

#[cfg(not(target_family = "wasm"))]
macro_rules! impl_protojson_payload {
    ($native:path, $proto:path, $protojson:path, $to_proto:path, $from_proto:path) => {
        impl ProtoJsonPayload for $native {
            type Proto = $proto;
            type ProtoJson = $protojson;

            fn to_proto(value: &Self) -> Self::Proto {
                $to_proto(value)
            }

            fn try_from_proto(value: &Self::Proto) -> Result<Self, ProtoJsonPayloadError> {
                Ok($from_proto(value))
            }
        }
    };
}

#[cfg(not(target_family = "wasm"))]
macro_rules! impl_protojson_payload_optional {
    ($native:path, $proto:path, $protojson:path, $to_proto:path, $from_proto:path) => {
        impl ProtoJsonPayload for $native {
            type Proto = $proto;
            type ProtoJson = $protojson;

            fn to_proto(value: &Self) -> Self::Proto {
                $to_proto(value)
            }

            fn try_from_proto(value: &Self::Proto) -> Result<Self, ProtoJsonPayloadError> {
                $from_proto(value).ok_or(ProtoJsonPayloadError::MissingPayload(stringify!($native)))
            }
        }
    };
}

// ---------------------------------------------------------------------------
// wasm32 targets: `proto` and `pbconv` are not available (tonic + mio).
// Use `protojson` types (pbjson OUT_DIR, tonic-free) for BOTH `Proto` and
// `ProtoJson`. `transcode_message` becomes a self-encode/decode round-trip
// through the same type — lossless because prost encode → decode is identity.
// `to_proto` / `try_from_proto` use serde JSON as the bridge (the `a2a` types
// have identical serde field names to the pbjson types via camelCase aliases).
// ---------------------------------------------------------------------------

#[cfg(target_family = "wasm")]
macro_rules! impl_protojson_payload {
    ($native:path, $protojson:path) => {
        impl ProtoJsonPayload for $native {
            // On wasm, Proto == ProtoJson: both are the pbjson-generated type.
            type Proto = $protojson;
            type ProtoJson = $protojson;

            fn to_proto(value: &Self) -> Self::Proto {
                // Bridge via JSON: serialize the hand-written `a2a` type, then
                // deserialize into the pbjson type (same camelCase field names).
                let v = serde_json::to_value(value).unwrap_or_default();
                serde_json::from_value(v).unwrap_or_default()
            }

            fn try_from_proto(value: &Self::Proto) -> Result<Self, ProtoJsonPayloadError> {
                let v = serde_json::to_value(value).map_err(ProtoJsonPayloadError::Json)?;
                serde_json::from_value(v).map_err(ProtoJsonPayloadError::Json)
            }
        }
    };
}

#[cfg(target_family = "wasm")]
macro_rules! impl_protojson_payload_optional {
    ($native:path, $protojson:path) => {
        impl ProtoJsonPayload for $native {
            type Proto = $protojson;
            type ProtoJson = $protojson;

            fn to_proto(value: &Self) -> Self::Proto {
                let v = serde_json::to_value(value).unwrap_or_default();
                serde_json::from_value(v).unwrap_or_default()
            }

            fn try_from_proto(value: &Self::Proto) -> Result<Self, ProtoJsonPayloadError> {
                let v = serde_json::to_value(value).map_err(ProtoJsonPayloadError::Json)?;
                serde_json::from_value(v).map_err(ProtoJsonPayloadError::Json)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// impl_protojson_payload! invocations — native uses 5 args, wasm uses 2.
// ---------------------------------------------------------------------------

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    SendMessageRequest,
    crate::pb::proto::SendMessageRequest,
    crate::pb::protojson::SendMessageRequest,
    crate::pb::pbconv::to_proto_send_message_request,
    crate::pb::pbconv::from_proto_send_message_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(SendMessageRequest, crate::pb::protojson::SendMessageRequest);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    GetTaskRequest,
    crate::pb::proto::GetTaskRequest,
    crate::pb::protojson::GetTaskRequest,
    crate::pb::pbconv::to_proto_get_task_request,
    crate::pb::pbconv::from_proto_get_task_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(GetTaskRequest, crate::pb::protojson::GetTaskRequest);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    ListTasksRequest,
    crate::pb::proto::ListTasksRequest,
    crate::pb::protojson::ListTasksRequest,
    crate::pb::pbconv::to_proto_list_tasks_request,
    crate::pb::pbconv::from_proto_list_tasks_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(ListTasksRequest, crate::pb::protojson::ListTasksRequest);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    CancelTaskRequest,
    crate::pb::proto::CancelTaskRequest,
    crate::pb::protojson::CancelTaskRequest,
    crate::pb::pbconv::to_proto_cancel_task_request,
    crate::pb::pbconv::from_proto_cancel_task_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(CancelTaskRequest, crate::pb::protojson::CancelTaskRequest);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    SubscribeToTaskRequest,
    crate::pb::proto::SubscribeToTaskRequest,
    crate::pb::protojson::SubscribeToTaskRequest,
    crate::pb::pbconv::to_proto_subscribe_to_task_request,
    crate::pb::pbconv::from_proto_subscribe_to_task_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    SubscribeToTaskRequest,
    crate::pb::protojson::SubscribeToTaskRequest
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    GetExtendedAgentCardRequest,
    crate::pb::proto::GetExtendedAgentCardRequest,
    crate::pb::protojson::GetExtendedAgentCardRequest,
    crate::pb::pbconv::to_proto_get_extended_agent_card_request,
    crate::pb::pbconv::from_proto_get_extended_agent_card_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    GetExtendedAgentCardRequest,
    crate::pb::protojson::GetExtendedAgentCardRequest
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    GetTaskPushNotificationConfigRequest,
    crate::pb::proto::GetTaskPushNotificationConfigRequest,
    crate::pb::protojson::GetTaskPushNotificationConfigRequest,
    crate::pb::pbconv::to_proto_get_task_push_notification_config_request,
    crate::pb::pbconv::from_proto_get_task_push_notification_config_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    GetTaskPushNotificationConfigRequest,
    crate::pb::protojson::GetTaskPushNotificationConfigRequest
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    DeleteTaskPushNotificationConfigRequest,
    crate::pb::proto::DeleteTaskPushNotificationConfigRequest,
    crate::pb::protojson::DeleteTaskPushNotificationConfigRequest,
    crate::pb::pbconv::to_proto_delete_task_push_notification_config_request,
    crate::pb::pbconv::from_proto_delete_task_push_notification_config_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    DeleteTaskPushNotificationConfigRequest,
    crate::pb::protojson::DeleteTaskPushNotificationConfigRequest
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    ListTaskPushNotificationConfigsRequest,
    crate::pb::proto::ListTaskPushNotificationConfigsRequest,
    crate::pb::protojson::ListTaskPushNotificationConfigsRequest,
    crate::pb::pbconv::to_proto_list_task_push_notification_configs_request,
    crate::pb::pbconv::from_proto_list_task_push_notification_configs_request
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    ListTaskPushNotificationConfigsRequest,
    crate::pb::protojson::ListTaskPushNotificationConfigsRequest
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    TaskPushNotificationConfig,
    crate::pb::proto::TaskPushNotificationConfig,
    crate::pb::protojson::TaskPushNotificationConfig,
    crate::pb::pbconv::to_proto_task_push_notification_config,
    crate::pb::pbconv::from_proto_task_push_notification_config
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    TaskPushNotificationConfig,
    crate::pb::protojson::TaskPushNotificationConfig
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    Task,
    crate::pb::proto::Task,
    crate::pb::protojson::Task,
    crate::pb::pbconv::to_proto_task,
    crate::pb::pbconv::from_proto_task
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(Task, crate::pb::protojson::Task);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    ListTasksResponse,
    crate::pb::proto::ListTasksResponse,
    crate::pb::protojson::ListTasksResponse,
    crate::pb::pbconv::to_proto_list_tasks_response,
    crate::pb::pbconv::from_proto_list_tasks_response
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(ListTasksResponse, crate::pb::protojson::ListTasksResponse);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    ListTaskPushNotificationConfigsResponse,
    crate::pb::proto::ListTaskPushNotificationConfigsResponse,
    crate::pb::protojson::ListTaskPushNotificationConfigsResponse,
    crate::pb::pbconv::to_proto_list_task_push_notification_configs_response,
    crate::pb::pbconv::from_proto_list_task_push_notification_configs_response
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(
    ListTaskPushNotificationConfigsResponse,
    crate::pb::protojson::ListTaskPushNotificationConfigsResponse
);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload!(
    AgentCard,
    crate::pb::proto::AgentCard,
    crate::pb::protojson::AgentCard,
    crate::pb::pbconv::to_proto_agent_card,
    crate::pb::pbconv::from_proto_agent_card
);
#[cfg(target_family = "wasm")]
impl_protojson_payload!(AgentCard, crate::pb::protojson::AgentCard);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload_optional!(
    SendMessageResponse,
    crate::pb::proto::SendMessageResponse,
    crate::pb::protojson::SendMessageResponse,
    crate::pb::pbconv::to_proto_send_message_response,
    crate::pb::pbconv::from_proto_send_message_response
);
#[cfg(target_family = "wasm")]
impl_protojson_payload_optional!(SendMessageResponse, crate::pb::protojson::SendMessageResponse);

#[cfg(not(target_family = "wasm"))]
impl_protojson_payload_optional!(
    StreamResponse,
    crate::pb::proto::StreamResponse,
    crate::pb::protojson::StreamResponse,
    crate::pb::pbconv::to_proto_stream_response,
    crate::pb::pbconv::from_proto_stream_response
);
#[cfg(target_family = "wasm")]
impl_protojson_payload_optional!(StreamResponse, crate::pb::protojson::StreamResponse);

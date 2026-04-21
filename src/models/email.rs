use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SendContactEmailRequest {
    pub email: String,
    pub firstname: String,
    pub lastname: String,
    #[serde(default)]
    pub phone: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SubscribeRequest {
    pub email: String,
}

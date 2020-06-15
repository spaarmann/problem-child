use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NotifChannel {
    pub id: u64,
    pub subscribed_users: Vec<u64>,
}

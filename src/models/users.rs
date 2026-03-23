use serde::{{Deserialize, Serialize}};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserKind { Registered, Guest }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tokens {
    pub access_token:  String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Users {
    #[serde(default = "new_v4_str")]
    pub id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    #[serde(default = "default_kind")]
    pub kind: UserKind,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub elo: u64
}

fn new_v4_str() -> String { Uuid::new_v4().to_string() }
fn default_kind() -> UserKind { UserKind::Registered }

impl Users {
    pub fn new(username: String, email: String, password_hash: String) -> Self {
        Self {
            id: new_v4_str(),
            username,
            email,
            password_hash,
            kind: UserKind::Registered,
            refresh_token: None,
            elo: 1000
        }
    }

    pub fn new_guest() -> Self {
        let n: u16 = rand::random();
        Self {
            id: format!("guest_{}", new_v4_str()),
            username: format!("Guest#{:04}", n),
            email: format!("{}@guest.com", new_v4_str()),
            password_hash: String::new(),
            kind: UserKind::Guest,
            refresh_token: None,
            elo: 1000
        }
    }
}
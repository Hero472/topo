use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MoveSuccess {
    ScalePlaced { scale_id: usize, completed: bool },
    ScaleOpened { scale_id: usize },
    Success,
    TurnEnded,
    GameWon { winner_id: usize },
}

impl MoveSuccess {
    pub fn turn_ended(&self) -> bool {
        matches!(self, MoveSuccess::TurnEnded | MoveSuccess::GameWon { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MoveError {
    DoesNotFit,
    NotYourTurn,
    NotAllowed,
    InvalidIndex { kind: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn turn_ended_returns_true_for_turn_ended() {
        let success = MoveSuccess::TurnEnded;
        assert!(success.turn_ended());
    }

    #[test]
    fn turn_ended_returns_true_for_game_won() {
        let success = MoveSuccess::GameWon { winner_id: 42 };
        assert!(success.turn_ended());
    }

    #[test]
    fn turn_ended_returns_false_for_scale_placed() {
        let success = MoveSuccess::ScalePlaced { scale_id: 1, completed: false };
        assert!(!success.turn_ended());
    }

    #[test]
    fn turn_ended_returns_false_for_scale_opened() {
        let success = MoveSuccess::ScaleOpened { scale_id: 2 };
        assert!(!success.turn_ended());
    }

    #[test]
    fn turn_ended_returns_false_for_success() {
        let success = MoveSuccess::Success;
        assert!(!success.turn_ended());
    }

    // -------------------------------------------------------------------------
    // Serialization / Deserialization tests for MoveSuccess
    // -------------------------------------------------------------------------

    #[test]
    fn move_success_scale_placed_roundtrip() {
        let original = MoveSuccess::ScalePlaced { scale_id: 3, completed: true };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveSuccess = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert!(json.contains(r#""type":"scale_placed""#));
        assert!(json.contains(r#""scale_id":3"#));
        assert!(json.contains(r#""completed":true"#));
    }

    #[test]
    fn move_success_scale_opened_roundtrip() {
        let original = MoveSuccess::ScaleOpened { scale_id: 5 };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveSuccess = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert!(json.contains(r#""type":"scale_opened""#));
        assert!(json.contains(r#""scale_id":5"#));
    }

    #[test]
    fn move_success_success_roundtrip() {
        let original = MoveSuccess::Success;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveSuccess = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert_eq!(json, r#"{"type":"success"}"#);
    }

    #[test]
    fn move_success_turn_ended_roundtrip() {
        let original = MoveSuccess::TurnEnded;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveSuccess = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert_eq!(json, r#"{"type":"turn_ended"}"#);
    }

    #[test]
    fn move_success_game_won_roundtrip() {
        let original = MoveSuccess::GameWon { winner_id: 7 };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveSuccess = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert!(json.contains(r#""type":"game_won""#));
        assert!(json.contains(r#""winner_id":7"#));
    }

    // -------------------------------------------------------------------------
    // Serialization / Deserialization tests for MoveError
    // -------------------------------------------------------------------------

    #[test]
    fn move_error_does_not_fit_roundtrip() {
        let original = MoveError::DoesNotFit;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveError = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert_eq!(json, r#"{"type":"does_not_fit"}"#);
    }

    #[test]
    fn move_error_not_your_turn_roundtrip() {
        let original = MoveError::NotYourTurn;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveError = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert_eq!(json, r#"{"type":"not_your_turn"}"#);
    }

    #[test]
    fn move_error_not_allowed_roundtrip() {
        let original = MoveError::NotAllowed;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveError = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert_eq!(json, r#"{"type":"not_allowed"}"#);
    }

    #[test]
    fn move_error_invalid_index_roundtrip() {
        let original = MoveError::InvalidIndex { kind: "scale".to_string() };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MoveError = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
        assert!(json.contains(r#""type":"invalid_index""#));
        assert!(json.contains(r#""kind":"scale""#));
    }
}
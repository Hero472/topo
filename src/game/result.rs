use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PlayResult {
    Ok { scale_id: usize, completed: bool },
    DoesNotFit,
    
    TurnEnded,
    Moved,
    Won,
    InvalidIndex,
    NotAllowed,
    NotYourTurn,
}
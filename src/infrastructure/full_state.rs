use crate::{
    core::game::{deck::DeckColor, state::GameState},
    infrastructure::{
        server_event::{OpponentView, ServerEvent},
        views::{PersonalPileView, PlayerBoardView}
    }
};

pub fn build_full_state(
    game_state: &GameState,
    player_id: usize,
    opponent_username: String,
) -> Option<ServerEvent> {
    let your_board = game_state.players.iter()
        .find(|p| p.player_idx == player_id)?;

    let personal_count = your_board.personal.len();
    let personal_top = your_board.personal_top().cloned();

    let colors: Vec<DeckColor> = your_board.personal.iter()
        .rev()                              // top is now first
        .map(|card| card.deck)
        .collect();

    let personal_view = PersonalPileView {
        count: personal_count,
        top: personal_top,
        colors,
    };

    let your_board_view = PlayerBoardView {
        player_idx: your_board.player_idx,
        personal: personal_view,
        side: your_board.side.clone(),
        hand: your_board.hand.clone(),
    };

    let opponent = game_state.players.iter()
        .find(|p| p.player_idx != player_id)
        .map(|opp| OpponentView {
            player_idx: opp.player_idx,
            username: opponent_username,
            hand_count: opp.hand_len(),
            personal_count: opp.personal.len(),
            personal_top: opp.personal_top().cloned(),
            side: opp.side.clone(),
        })
        .unwrap_or_else(|| OpponentView {
            player_idx: 0,
            username: String::new(),
            hand_count: 5,
            personal_count: 0,
            personal_top: None,
            side: [vec![], vec![], vec![], vec![]],
        });

    let draw_top = game_state.card_dealer.peek().clone();
    let color_top = draw_top.map(|card| card.deck);

    println!("current_id: {:?}, player_id: {:?}, current_turn {:?}", game_state.current_player_id(), Some(player_id), game_state.current_turn);

    Some(ServerEvent::FullState {
        your_board: your_board_view,
        your_turn: game_state.current_player_id() == Some(player_id),
        opponent,
        scales: game_state.scale_manager.scales.clone(),
        dealer_top: color_top,
        dealer_count: game_state.card_dealer.draw_pile.remaining(),
        turn_seconds_remaining: game_state.turn_seconds,
    })
}
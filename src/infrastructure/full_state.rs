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

    let your_turn = game_state
        .players
        .get(game_state.current_turn)
        .map(|p| p.player_idx)
        .unwrap_or(0) == player_id;

    println!(
        "[FULL_STATE] player_id={}, current_turn={}, your_turn={}",
        player_id, game_state.current_turn, your_turn
    );

    Some(ServerEvent::FullState {
        your_board: your_board_view,
        your_turn: game_state
            .players
            .get(game_state.current_turn)
            .map(|p| p.player_idx)
            .unwrap_or(0) == player_id,
        opponent,
        scales: game_state.scale_manager.scales.clone(),
        dealer_top: color_top,
        dealer_count: game_state.card_dealer.draw_pile.remaining(),
        turn_seconds_remaining: game_state.turn_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game::board::PlayerBoard;
    use crate::core::game::card::{Card, Suit};
    use crate::core::game::dealer::CardDealer;
    use crate::core::game::deck::DeckColor;
    use crate::core::game::state::GameState;

    // Helper: create a dummy Card with a given color.
    // Fill all required Card fields with dummy values.
    fn dummy_card(deck: DeckColor) -> Card {
        Card {
            deck,
            suit: Suit::Clubs,
            value: 3
        }
    }

    fn make_player_board(idx: usize, personal: Vec<Card>, hand: Vec<Card>) -> PlayerBoard {
        let mut board = PlayerBoard::new(idx);
        board.set_personal(personal);
        board.hand = hand;
        board
    }

    fn make_test_game_state(
        players: Vec<PlayerBoard>,
        current_turn: usize,
        draw_pile_top: Option<Card>,
        draw_pile_remaining: usize,
        turn_seconds: u64,
    ) -> GameState {
        let mut card_dealer = CardDealer::new(0);

        let mut cards = Vec::with_capacity(draw_pile_remaining);
        if let Some(top) = draw_pile_top {
            for _ in 0..(draw_pile_remaining - 1) {
                cards.push(dummy_card(DeckColor::Blue));
            }
            cards.push(top);
        } else {
            for _ in 0..draw_pile_remaining {
                cards.push(dummy_card(DeckColor::Blue));
            }
        }
        card_dealer.draw_pile.cards = cards;

        GameState::test_new(
            players,
            current_turn,
            card_dealer,
            turn_seconds,
        )
    }

    // -------------------------------------------------------------------------
    // The actual tests
    // -------------------------------------------------------------------------

    #[test]
    fn player_not_found_returns_none() {
        let state = make_test_game_state(vec![], 0, None, 0, 30);
        let result = build_full_state(&state, 42, "opponent".to_string());
        assert!(result.is_none());
    }

    #[test]
    fn success_path_with_both_players() {
        let personal_cards = vec![
            dummy_card(DeckColor::Red),
            dummy_card(DeckColor::Blue),
            dummy_card(DeckColor::Red),
        ];
        let hand_cards = vec![dummy_card(DeckColor::Red)];

        let your_board = make_player_board(1, personal_cards.clone(), hand_cards.clone());
        let opponent_board = make_player_board(2, vec![], vec![]);

        let state = make_test_game_state(
            vec![your_board, opponent_board],
            0,
            Some(dummy_card(DeckColor::Blue)),
            15,
            42,
        );

        let result = build_full_state(&state, 1, "Alice".to_string());
        assert!(result.is_some());

        if let Some(ServerEvent::FullState { your_board, your_turn, opponent, dealer_top, dealer_count, turn_seconds_remaining, .. }) = result {
            assert_eq!(your_board.player_idx, 1);
            assert_eq!(your_board.personal.count, 3);
            assert_eq!(your_board.personal.top, personal_cards.last().cloned());

            let expected_colors = vec![DeckColor::Red, DeckColor::Blue, DeckColor::Red];
            assert_eq!(your_board.personal.colors, expected_colors);
            assert_eq!(your_board.hand, hand_cards);

            assert!(your_turn);

            assert_eq!(opponent.player_idx, 2);
            assert_eq!(opponent.username, "Alice");
            assert_eq!(opponent.hand_count, 0);
            assert_eq!(opponent.personal_count, 0);
            assert_eq!(opponent.personal_top, None);

            assert_eq!(dealer_top, Some(DeckColor::Blue));
            assert_eq!(dealer_count, 15);
            assert_eq!(turn_seconds_remaining, 42);
        } else {
            panic!("Wrong ServerEvent variant");
        }
    }

    #[test]
    fn your_turn_false_when_not_current_player() {
        let your_board = make_player_board(1, vec![], vec![]);
        let opponent_board = make_player_board(2, vec![], vec![]);
        let state = make_test_game_state(vec![your_board, opponent_board], 1, None, 0, 30);
        let result = build_full_state(&state, 1, "Bob".to_string());
        if let Some(ServerEvent::FullState { your_turn, .. }) = result {
            assert!(!your_turn);
        } else {
            panic!("Expected FullState");
        }
    }

    #[test]
    fn empty_personal_pile() {
        let your_board = make_player_board(1, vec![], vec![]);
        let opponent_board = make_player_board(2, vec![], vec![]);
        let state = make_test_game_state(vec![your_board, opponent_board], 1, None, 10, 5);
        let result = build_full_state(&state, 1, "Charlie".to_string());
        if let Some(ServerEvent::FullState { your_board, .. }) = result {
            assert_eq!(your_board.personal.count, 0);
            assert_eq!(your_board.personal.top, None);
            assert!(your_board.personal.colors.is_empty());
        } else {
            panic!("Expected FullState");
        }
    }

    #[test]
    fn dealer_top_none_when_draw_pile_empty() {
        let your_board = make_player_board(1, vec![], vec![]);
        let opponent_board = make_player_board(2, vec![], vec![]);
        let state = make_test_game_state(vec![your_board, opponent_board], 1, None, 0, 10);
        let result = build_full_state(&state, 1, "Dave".to_string());
        if let Some(ServerEvent::FullState { dealer_top, .. }) = result {
            assert_eq!(dealer_top, None);
        } else {
            panic!("Expected FullState");
        }
    }

    #[test]
    fn opponent_not_found_uses_default() {
        let your_board = make_player_board(1, vec![], vec![]);
        let state = make_test_game_state(vec![your_board], 1, None, 5, 20);
        let result = build_full_state(&state, 1, "Eve".to_string());
        if let Some(ServerEvent::FullState { opponent, .. }) = result {
            assert_eq!(opponent.player_idx, 0);
            assert_eq!(opponent.username, String::new());
            assert_eq!(opponent.hand_count, 5);
            assert_eq!(opponent.personal_count, 0);
            assert_eq!(opponent.personal_top, None);
            assert_eq!(opponent.side, [vec![], vec![], vec![], vec![]]);
        } else {
            panic!("Expected FullState");
        }
    }
}
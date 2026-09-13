use crate::{
    core::{game::state::GameState, game_id::GameId, player::{PlayerId, PlayerIdx}}, infrastructure::{
        room::player_info::PlayerInfo, server_event::{FullState, LobbyPlayerView, OpponentView, RoomSnapshot, ServerEvent}, views::{PersonalPileView, PlayerBoardView}
    }
};

const PERSONAL_PREVIEW_SIZE: usize = 7;
const DEALER_PREVIEW_SIZE: usize = 7;

pub fn build_lobby_full_state(
    game_id: GameId,
    player_id: PlayerId,
    player_idx: PlayerIdx,
    players_info: &std::collections::HashMap<PlayerId, PlayerInfo>, 
) -> ServerEvent {
    let players_view = players_info
        .values()
        .map(|info| LobbyPlayerView {
            player_id: info.player_id,
            player_idx: info.player_idx,
            username: info.username.clone(),
            is_ready: info.is_ready,
            is_disconnected: !info.connected,
        })
        .collect();

    ServerEvent::FullState {
        state: FullState {
            player_id,
            player_idx,
            snapshot: RoomSnapshot::Lobby {
                game_id,
                players: players_view,
            },
        },
    }
}

pub fn build_full_state(
    game_state: &GameState,
    player_id: PlayerId,
    player_idx: PlayerIdx
) -> Option<ServerEvent> {

    let your_board_state = game_state
        .players
        .iter()
        .find(|p| p.player_idx == player_idx)?;

    let personal_view = PersonalPileView {
        count: your_board_state.personal.len(),
        top: your_board_state.personal_top().cloned(),
        colors: your_board_state
            .personal
            .iter()
            .rev()
            .skip(1)
            .take(PERSONAL_PREVIEW_SIZE)
            .map(|card| card.deck)
            .collect(),
    };

    let your_board_view = PlayerBoardView {
        player_idx: your_board_state.player_idx,
        personal: personal_view,
        side: your_board_state.side.clone(),
        hand: your_board_state.hand.clone(),
    };

    let opponent = game_state
        .players
        .iter()
        .find(|p| p.player_idx != player_idx)
        .map(|opp| OpponentView {
            player_idx: opp.player_idx,
            username: opp.username.clone(),
            hand: opp.hand.iter().map(|card| card.dummy_card()).collect(),
            personal_count: opp.personal.len(),
            personal_top: opp.personal_top().cloned(),
            side: opp.side.clone(),
        })
        .unwrap_or_else(|| OpponentView {
            player_idx: PlayerIdx(0),
            username: String::new(),
            hand: vec![],
            personal_count: 0,
            personal_top: None,
            side: [vec![], vec![], vec![], vec![]],
        });

    let dealer_preview = game_state
        .card_dealer
        .draw_pile
        .iter()
        .rev()
        .take(DEALER_PREVIEW_SIZE)
        .map(|card| card.deck)
        .collect();

    let scales = game_state
        .scale_manager
        .scales
        .iter()
        .cloned()
        .collect();

    let is_your_turn = game_state
        .players
        .get(game_state.current_turn.as_usize())
        .map(|p| p.player_idx)
        .unwrap_or(PlayerIdx(0))
        == player_idx;

    Some(ServerEvent::FullState {
        state: FullState {
            player_id,
            player_idx,
            snapshot: RoomSnapshot::Playing {
                your_board: your_board_view,
                your_turn: is_your_turn,
                opponent,
                scales,
                dealer_preview,
                dealer_count: game_state.card_dealer.draw_pile.remaining(),
                turn_seconds_remaining: game_state.turn_seconds,
            },
        },
    })
}

pub fn build_game_over_full_state(
    player_id: PlayerId,
    player_idx: PlayerIdx,
    winner_id: PlayerId,
    winner_idx: PlayerIdx,
    reason: String,
) -> ServerEvent {
    ServerEvent::FullState {
        state: FullState {
            player_id,
            player_idx,
            snapshot: RoomSnapshot::GameOver {
                winner_id,
                winner_idx,
                reason,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::core::game::board::PlayerBoard;
    use crate::core::game::card::{Card, Suit};
    use crate::core::game::dealer::CardDealer;
    use crate::core::game::deck::DeckColor;
    use crate::core::game::state::{GameState, Seconds};
    use crate::core::game::state::state_types::Seed;
    use crate::core::player::PlayerId;

    fn dummy_card(deck: DeckColor) -> Card {
        Card {
            deck,
            suit: Suit::Clubs,
            value: 3
        }
    }

    fn make_player_board(
        player_idx: PlayerIdx,
        username: &str,
        personal: Vec<Card>,
        hand: Vec<Card>,
    ) -> PlayerBoard {
        let mut board = PlayerBoard::new(player_idx, username.to_string());
        board.player_id = Some(PlayerId(Uuid::nil()));
        board.set_personal(personal);
        board.hand = hand;
        board
    }

    fn make_test_game_state(
        players: Vec<PlayerBoard>,
        current_turn: PlayerIdx,
        draw_pile_top: Option<Card>,
        draw_pile_remaining: usize,
        turn_seconds: Seconds,
    ) -> GameState {
        let mut card_dealer = CardDealer::new(Seed(0));

        // Fake the draw pile: push dummy cards then the top card if any.
        let mut cards = Vec::with_capacity(draw_pile_remaining);
        if let Some(top) = draw_pile_top {
            for _ in 0..(draw_pile_remaining.saturating_sub(1)) {
                cards.push(dummy_card(DeckColor::Blue));
            }
            cards.push(top);
        } else {
            for _ in 0..draw_pile_remaining {
                cards.push(dummy_card(DeckColor::Blue));
            }
        }
        card_dealer.draw_pile.cards = cards;

        GameState::test_new(players, current_turn, card_dealer, turn_seconds)
    }

    // -------------------------------------------------------------------------
    // The actual tests
    // -------------------------------------------------------------------------

    #[test]
    fn player_not_found_returns_none() {
        let state = make_test_game_state(
            vec![],
            PlayerIdx(0),
            None,
            0,
            Seconds(30),
        );
        // Pass PlayerIdx(42) – no such player in state
        let result = build_full_state(&state, PlayerId(Uuid::nil()), PlayerIdx(42));
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

        let your_board = make_player_board(PlayerIdx(1), "You", personal_cards.clone(), hand_cards.clone());
        let opponent_board = make_player_board(PlayerIdx(2), "Alice", vec![], vec![]);

        let state = make_test_game_state(
            vec![your_board, opponent_board],
            PlayerIdx(0),
            Some(dummy_card(DeckColor::Blue)),
            15,
            Seconds(42),
        );

        let result = build_full_state(&state, PlayerId(Uuid::nil()), PlayerIdx(1));
        assert!(result.is_some());

        if let Some(ServerEvent::FullState { state: full_state }) = result {
            if let RoomSnapshot::Playing {
                your_board,
                your_turn,
                opponent,
                dealer_preview,
                dealer_count,
                turn_seconds_remaining,
                ..
            } = full_state.snapshot
            {
                assert_eq!(your_board.player_idx, PlayerIdx(1));
                assert_eq!(your_board.personal.count, 3);
                assert_eq!(your_board.personal.top, personal_cards.last().cloned());

                let expected_colors = vec![DeckColor::Red, DeckColor::Blue, DeckColor::Red];
                assert_eq!(your_board.personal.colors, expected_colors);
                assert_eq!(your_board.hand, hand_cards);

                assert!(your_turn);

                assert_eq!(opponent.player_idx, PlayerIdx(2));
                assert_eq!(opponent.username, "Alice"); // Now correctly matches the test setup
                assert_eq!(opponent.hand.len(), 0);
                assert_eq!(opponent.personal_count, 0);
                assert_eq!(opponent.personal_top, None);

                assert_eq!(dealer_preview, vec![DeckColor::Blue]);
                assert_eq!(dealer_count, 15);
                assert_eq!(turn_seconds_remaining, Seconds(42));
            } else {
                panic!("Expected RoomSnapshot::Playing");
            }
        } else {
            panic!("Wrong ServerEvent variant");
        }
    }

    #[test]
    fn your_turn_false_when_not_current_player() {
        let your_board = make_player_board(PlayerIdx(1), "You", vec![], vec![]);
        let opponent_board = make_player_board(PlayerIdx(2), "Bob", vec![], vec![]);
        let state = make_test_game_state(
            vec![your_board, opponent_board],
            PlayerIdx(1),
            None,
            0,
            Seconds(30),
        );
        let result = build_full_state(&state, PlayerId(Uuid::nil()), PlayerIdx(1));
        
        if let Some(ServerEvent::FullState { state: full_state }) = result {
            if let RoomSnapshot::Playing { your_turn, .. } = full_state.snapshot {
                assert!(!your_turn);
            } else {
                panic!("Expected RoomSnapshot::Playing");
            }
        } else {
            panic!("Expected FullState");
        }
    }

    #[test]
    fn empty_personal_pile() {
        let your_board = make_player_board(PlayerIdx(1), "You", vec![], vec![]);
        let opponent_board = make_player_board(PlayerIdx(2), "Charlie", vec![], vec![]);
        let state = make_test_game_state(
            vec![your_board, opponent_board],
            PlayerIdx(1),
            None,
            10,
            Seconds(5),
        );
        let result = build_full_state(&state, PlayerId(Uuid::nil()), PlayerIdx(1));
        
        if let Some(ServerEvent::FullState { state: full_state }) = result {
            if let RoomSnapshot::Playing { your_board, .. } = full_state.snapshot {
                assert_eq!(your_board.personal.count, 0);
                assert_eq!(your_board.personal.top, None);
                assert!(your_board.personal.colors.is_empty());
            } else {
                panic!("Expected RoomSnapshot::Playing");
            }
        } else {
            panic!("Expected FullState");
        }
    }

    #[test]
    fn dealer_top_none_when_draw_pile_empty() {
        let your_board = make_player_board(PlayerIdx(1), "You", vec![], vec![]);
        let opponent_board = make_player_board(PlayerIdx(2), "Dave", vec![], vec![]);
        let state = make_test_game_state(
            vec![your_board, opponent_board],
            PlayerIdx(1),
            None,
            0,
            Seconds(10),
        );
        let result = build_full_state(&state, PlayerId(Uuid::nil()), PlayerIdx(1));
        
        if let Some(ServerEvent::FullState { state: full_state }) = result {
            if let RoomSnapshot::Playing { dealer_preview, .. } = full_state.snapshot {
                assert_eq!(dealer_preview, vec![]);
            } else {
                panic!("Expected RoomSnapshot::Playing");
            }
        } else {
            panic!("Expected FullState");
        }
    }

    #[test]
    fn opponent_not_found_uses_default() {
        let your_board = make_player_board(PlayerIdx(1), "You", vec![], vec![]);
        let state = make_test_game_state(vec![your_board], PlayerIdx(1), None, 5, Seconds(20));
        
        let result = build_full_state(&state, PlayerId(Uuid::nil()), PlayerIdx(1));
        
        if let Some(ServerEvent::FullState { state: full_state }) = result {
            if let RoomSnapshot::Playing { opponent, .. } = full_state.snapshot {
                assert_eq!(opponent.player_idx, PlayerIdx(0)); // default placeholder
                assert_eq!(opponent.username, String::new());
                assert_eq!(opponent.hand.len(), 0);
                assert_eq!(opponent.personal_count, 0);
                assert_eq!(opponent.personal_top, None);
                assert_eq!(opponent.side, [vec![], vec![], vec![], vec![]]);
            } else {
                panic!("Expected RoomSnapshot::Playing");
            }
        } else {
            panic!("Expected FullState");
        }
    }
}
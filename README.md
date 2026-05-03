# topo

src/
├── main.rs                  # entry point, wires everything together
├── lib.rs                   # (optional) re‑exports for integration tests
│
├── core/                    # pure domain logic — no HTTP, no DB, no Tokio
│   ├── mod.rs
│   └── game/                # the actual card game
│       ├── mod.rs
│       ├── card/
│       │   ├── mod.rs
│       │   ├── suit.rs
│       │   ├── color.rs
│       │   └── card.rs
│       ├── deck/
│       │   ├── mod.rs
│       │   └── deck.rs
│       ├── board/
│       │   ├── mod.rs
│       │   └── player_board.rs
│       ├── actions/
│       │   ├── mod.rs
│       │   ├── action.rs
│       │   ├── result.rs
│       │   └── phase.rs
│       ├── scale/
│       │   ├── mod.rs
│       │   ├── scale.rs
│       │   └── scale_manager.rs
│       ├── dealer/
│       │   ├── mod.rs
│       │   └── card_dealer.rs
│       └── state/
│           ├── mod.rs
│           └── game_state.rs
│
├── infrastructure/          # external services, side‑effects, technical glue
│   ├── mod.rs
│   ├── timer.rs             # start_turn_timer (uses Tokio)
│   └── server_event.rs      # the ServerEvent enum (wire protocol)
│
├── api/                     # web layer — the server’s entry points
│   ├── mod.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── game.rs          # POST /game/create, GET /game/{id}, etc.
│   │   └── health.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   └── game.rs          # thin async functions that call core logic
│   ├── models/
│   │   ├── mod.rs
│   │   └── game.rs          # request/response DTOs (serde)
│   ├── state.rs             # AppState (shared rooms, etc.)
│   └── error.rs             # HTTP error conversions
│
└── utils/                   # (optional) completely generic helpers
    └── mod.rs               # e.g., short uuid, logging setup
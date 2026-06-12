# convergence.json schema

Emitted by the trainer every `--convergence-interval` iterations (and on
shutdown) so that Phase 7's per-tier runner can graph convergence and
decide when to stop.

## Schema (1.0)

```json
{
  "schema_version": "1.0",
  "iter": 1000000,
  "t_seconds": 145.2,
  "depth_tier_bb": 20,
  "n_players": 2,
  "ev": [0.012, -0.012],
  "best_response": [0.045, 0.040],
  "exploitability_mbb_per_hand": [4.5, 4.0],
  "exploitability_max_mbb_per_hand": 4.5,
  "memory_mb": 1820,
  "n_threads": 16,
  "stop_reason": null
}
```

## Field reference

| Field | Type | Meaning |
|---|---|---|
| `schema_version` | string | `"1.0"`. Future-proofs reads. |
| `iter` | integer | Iteration count when this sample was taken. |
| `t_seconds` | float | Wall-clock seconds since training started. |
| `depth_tier_bb` | integer | Stack depth tier (one of {5, 8, 10, 12, 15, 18, 20, 25}). |
| `n_players` | integer | 2 or 3. |
| `ev` | float[] | Average EV for each player under the current average strategy. Length = n_players. |
| `best_response` | float[] | Per-player best response EV. Length = n_players. |
| `exploitability_mbb_per_hand` | float[] | `best_response[i] - ev[i]` for each player, in milli-big-blinds per hand. Length = n_players. |
| `exploitability_max_mbb_per_hand` | float | `max(exploitability_mbb_per_hand)`. The headline convergence metric. |
| `memory_mb` | integer | Approximate RSS in megabytes. |
| `n_threads` | integer | Number of MCCFR worker threads. |
| `stop_reason` | string? | One of: `null` (still training), `"target_reached"`, `"max_iter"`, `"oom"`, `"error"`. |

## Conventions

- **Constant-sum 3p**: `sum(ev[i]) == 0` by convention. The MCCFR
  trainer outputs EVs in chip units; for 3p the convention is that the
  first two players split the winnings and the third takes the negative
  of the average. Real 3p EV output requires the constant-sum
  convention to be enforced at the trainer.
- **Exploitability vs convergence**: the 2p `exploitability_max_mbb_per_hand`
  is a tight bound on "how exploitable is the current strategy?" — at 0
  the strategy is a Nash equilibrium. In 3p, this is a *conservative
  bound* (each player's BR is computed against a *uniform* opponent
  mixture; the true best response to any other strategy may be lower).
  See TASKS.md and the 3p blueprint discussion for details.
- **2p trivial check**: for heads-up postflop, the literature target is
  <5 mbb/h on a 20bb turn-river scenario within a few hours of
  training. The Phase 5 checkpoint uses this as a success bar.

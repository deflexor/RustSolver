#!/usr/bin/env bash
# Phase 10/12 quality gates: KK turn + HU turn suite (geometry, parity, speed, exploitability).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export OUT_DIR="${OUT_DIR:-$ROOT/target/release/deps}"

if [[ ! -f "$OUT_DIR/offset_table.dat" ]]; then
  echo "Building solver (offset_table.dat)..."
  cargo build --release
fi

echo "=== KK turn quality gate (rust) ==="
cargo test --release -p rust_solver kk_turn_quality_gate -- --ignored

echo "=== Exploitability scale unit test (small tree, G4 <50) ==="
cargo test --release -p rust_solver turn_tree_exploitability_under_budget

echo "=== HU turn suite quality gate (rust) ==="
cargo test --release -p rust_solver hu_turn_suite_quality_gate -- --ignored

if [[ -d "$ROOT/.venv" ]]; then
  # shellcheck disable=SC1091
  source "$ROOT/.venv/bin/activate"
  if python -c "import rust_solver_py" 2>/dev/null; then
    echo "=== Python import smoke ==="
    OUT_DIR="$OUT_DIR" python -c "
import rust_solver_py as rs
s = rs.SolverSession('flop')
samples = s.solve_flop_tree(
    hero_hand='KsKc', spot='HU_CX', stack=12, flop='4dQcQd', weip_flop=False,
    bet_sizes='50%, 75%, 100%, a', turn_card_limit=2, max_iter=1000,
    oop_range='66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s',
    ip_range='QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+',
)
turn = [x for x in samples if x.street == 'turn' and abs(x.pot_bb - 2.0) < 0.5 and x.call_cost_bb < 0.01]
assert turn, 'no turn sample at pot=2 call=0'
best = min(turn, key=lambda x: abs(x.pot_bb - 2.0) + abs(x.call_cost_bb))
check = best.action_probs[1]
print(f'python check={check:.4f} pot={best.pot_bb:.2f}')
assert 0.45 <= check <= 0.75, f'check out of parity band: {check}'
print('python gate OK')
"
  fi
fi

echo "All quality gates passed."

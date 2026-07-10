//! PyO3 bindings — `solver_ext`-compatible `TrainingSample` + `SolverSession`.

use pyo3::prelude::*;
use rust_poker::hand_evaluator::init_cards;
use rust_solver::python_api::{
    solve_flop_tree, SolveFlopTreeConfig, TrainingSample as RustSample,
};

fn ensure_evaluator() {
    if std::env::var("OUT_DIR").is_err() {
        let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/release/deps");
        if candidate.join("offset_table.dat").exists() {
            std::env::set_var("OUT_DIR", &candidate);
        }
    }
    init_cards();
}

#[pyclass]
#[derive(Clone)]
struct TrainingSample {
    hero_hole: Vec<String>,
    board: Vec<String>,
    street: String,
    hero_pos: String,
    weip_flop: bool,
    pot_bb: f32,
    eff_stack_bb: f32,
    call_cost_bb: f32,
    min_raise_to_bb: f32,
    max_raise_to_bb: f32,
    action_probs: Vec<f32>,
    raise_probs: Vec<f32>,
    history_players: Vec<u8>,
    history_actions: Vec<String>,
    history_sizes: Vec<f32>,
}

impl From<RustSample> for TrainingSample {
    fn from(s: RustSample) -> Self {
        TrainingSample {
            hero_hole: s.hero_hole,
            board: s.board,
            street: s.street,
            hero_pos: s.hero_pos,
            weip_flop: s.weip_flop,
            pot_bb: s.pot_bb,
            eff_stack_bb: s.eff_stack_bb,
            call_cost_bb: s.call_cost_bb,
            min_raise_to_bb: s.min_raise_to_bb,
            max_raise_to_bb: s.max_raise_to_bb,
            action_probs: s.action_probs,
            raise_probs: s.raise_probs,
            history_players: s.history_players,
            history_actions: s.history_actions,
            history_sizes: s.history_sizes,
        }
    }
}

#[pymethods]
impl TrainingSample {
    #[getter]
    fn hero_hole(&self) -> Vec<String> {
        self.hero_hole.clone()
    }
    #[getter]
    fn board(&self) -> Vec<String> {
        self.board.clone()
    }
    #[getter]
    fn street(&self) -> &str {
        &self.street
    }
    #[getter]
    fn hero_pos(&self) -> &str {
        &self.hero_pos
    }
    #[getter]
    fn weip_flop(&self) -> bool {
        self.weip_flop
    }
    #[getter]
    fn pot_bb(&self) -> f32 {
        self.pot_bb
    }
    #[getter]
    fn eff_stack_bb(&self) -> f32 {
        self.eff_stack_bb
    }
    #[getter]
    fn call_cost_bb(&self) -> f32 {
        self.call_cost_bb
    }
    #[getter]
    fn min_raise_to_bb(&self) -> f32 {
        self.min_raise_to_bb
    }
    #[getter]
    fn max_raise_to_bb(&self) -> f32 {
        self.max_raise_to_bb
    }
    #[getter]
    fn action_probs(&self) -> Vec<f32> {
        self.action_probs.clone()
    }
    #[getter]
    fn raise_probs(&self) -> Vec<f32> {
        self.raise_probs.clone()
    }
    #[getter]
    fn history_players(&self) -> Vec<u8> {
        self.history_players.clone()
    }
    #[getter]
    fn history_actions(&self) -> Vec<String> {
        self.history_actions.clone()
    }
    #[getter]
    fn history_sizes(&self) -> Vec<f32> {
        self.history_sizes.clone()
    }

    fn check_valid(&self) -> Option<String> {
        let rs = RustSample {
            hero_hole: self.hero_hole.clone(),
            board: self.board.clone(),
            street: self.street.clone(),
            hero_pos: self.hero_pos.clone(),
            weip_flop: self.weip_flop,
            pot_bb: self.pot_bb,
            eff_stack_bb: self.eff_stack_bb,
            call_cost_bb: self.call_cost_bb,
            min_raise_to_bb: self.min_raise_to_bb,
            max_raise_to_bb: self.max_raise_to_bb,
            action_probs: self.action_probs.clone(),
            raise_probs: self.raise_probs.clone(),
            history_players: self.history_players.clone(),
            history_actions: self.history_actions.clone(),
            history_sizes: self.history_sizes.clone(),
        };
        match rs.validate() {
            Ok(()) => None,
            Err(e) => Some(e),
        }
    }
}

/// Session holder (`solver_ext.SolverSession` compat). `flop_dir` is ignored for v1.
#[pyclass]
struct SolverSession {
    _flop_dir: String,
}

#[pymethods]
impl SolverSession {
    #[new]
    fn new(flop_dir: &str) -> Self {
        SolverSession {
            _flop_dir: flop_dir.to_string(),
        }
    }

    /// List turn cards remaining for a flop (49 minus flop cards).
    fn list_turn_cards(&self, flop: &str) -> PyResult<Vec<String>> {
        Ok(rust_solver::python_api::list_turn_cards_public(flop))
    }

    /// Solve turn/river trees for sampled turn cards (MCCFR).
    #[pyo3(signature = (
        hero_hand,
        spot,
        stack,
        flop,
        weip_flop,
        max_iter=None,
        target_frac=None,
        bet_sizes=None,
        use_donk=None,
        use_compression=None,
        turn_card_limit=None,
        oop_range=None,
        ip_range=None,
    ))]
    #[allow(unused_variables)]
    fn solve_flop_tree(
        &self,
        hero_hand: &str,
        spot: &str,
        stack: u32,
        flop: &str,
        weip_flop: bool,
        max_iter: Option<u32>,
        target_frac: Option<f32>,
        bet_sizes: Option<&str>,
        use_donk: Option<bool>,
        use_compression: Option<bool>,
        turn_card_limit: Option<u32>,
        oop_range: Option<&str>,
        ip_range: Option<&str>,
    ) -> PyResult<Vec<TrainingSample>> {
        ensure_evaluator();
        let cfg = SolveFlopTreeConfig {
            hero_hand: hero_hand.to_string(),
            stack_bb: stack,
            flop: flop.to_string(),
            weip_flop,
            max_iter: max_iter.unwrap_or(200) as usize,
            turn_card_limit: turn_card_limit.unwrap_or(2) as usize,
            turn_cards: None,
            oop_range: oop_range.map(String::from),
            ip_range: ip_range.map(String::from),
            turn_pot_chips: None,
            n_threads: 1,
        };
        let result = solve_flop_tree(&cfg);
        Ok(result.samples.into_iter().map(TrainingSample::from).collect())
    }
}

/// Minimal one-shot turn decision API.
#[pyfunction]
#[pyo3(signature = (
    hero_hand,
    flop,
    turn_card,
    stack_bb=12,
    pot_bb=6.16,
    call_cost_bb=0.0,
    weip_flop=false,
    max_iter=200,
    oop_range=None,
    ip_range=None,
))]
#[allow(unused_variables)]
fn solve_turn_decision(
    hero_hand: &str,
    flop: &str,
    turn_card: &str,
    stack_bb: u32,
    pot_bb: f32,
    call_cost_bb: f32,
    weip_flop: bool,
    max_iter: u32,
    oop_range: Option<&str>,
    ip_range: Option<&str>,
) -> PyResult<TrainingSample> {
    ensure_evaluator();
    let cfg = SolveFlopTreeConfig {
        hero_hand: hero_hand.to_string(),
        stack_bb,
        flop: flop.to_string(),
        weip_flop,
        max_iter: max_iter as usize,
        turn_card_limit: 1,
        turn_cards: Some(vec![turn_card.to_string()]),
        oop_range: oop_range.map(String::from),
        ip_range: ip_range.map(String::from),
        turn_pot_chips: Some((pot_bb * 100.0).round() as u32),
        n_threads: 1,
    };
    let mut result = solve_flop_tree(&cfg);
    let query_pot = pot_bb;
    let query_call = call_cost_bb;
    result.samples.sort_by(|a, b| {
        let da = (a.pot_bb - query_pot).abs() + (a.call_cost_bb - query_call).abs();
        let db = (b.pot_bb - query_pot).abs() + (b.call_cost_bb - query_call).abs();
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    result
        .samples
        .into_iter()
        .find(|s| s.street == "turn")
        .map(TrainingSample::from)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("no turn decision sample found")
        })
}

#[pymodule]
fn rust_solver_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TrainingSample>()?;
    m.add_class::<SolverSession>()?;
    m.add_function(wrap_pyfunction!(solve_turn_decision, m)?)?;
    Ok(())
}

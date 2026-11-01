//! All tuning constants of the simulation. Defaults live in code; `data/config.ron` may override any field.
//!
//! Tactical parameters (`TacticsConfig`) are named after their principle ID in `docs/tactics/principles.md`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SimConfig {
    pub timing: TimingConfig,
    pub ball: BallConfig,
    pub player: PlayerConfig,
    pub tactics: TacticsConfig,
    pub ai: AiConfig,
    pub personality: PersonalityConfig,
    pub execution: ExecutionConfig,
    pub duel: DuelConfig,
    pub rules: RulesConfig,
}

impl SimConfig {
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TimingConfig {
    /// Simulation tick length (50 Hz).
    pub dt: f32,
    /// Ball physics sub-steps per tick.
    pub ball_substeps: u32,
    /// Length of one half in seconds.
    pub half_length_s: f32,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self { dt: 0.02, ball_substeps: 4, half_length_s: 45.0 * 60.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BallConfig {
    pub gravity: f32,
    pub air_density: f32,
    pub drag_coefficient: f32,
    /// Magnus acceleration = magnus_coeff · (ω × v).
    pub magnus_coeff: f32,
    /// Exponential spin decay rates (1/s).
    pub spin_decay_air: f32,
    pub spin_decay_ground: f32,
    /// Bounce restitution on a soft and on a hard pitch (interpolated by firmness).
    pub restitution_soft: f32,
    pub restitution_hard: f32,
    /// Fraction of restitution lost on a fully wet pitch.
    pub restitution_wet_loss: f32,
    /// Coulomb friction during a bounce on a dry and on a wet pitch (wet balls skid).
    pub bounce_friction_dry: f32,
    pub bounce_friction_wet: f32,
    /// Rolling resistance coefficient on short and on long grass; reduced on wet grass.
    pub rolling_resistance_short: f32,
    pub rolling_resistance_long: f32,
    pub rolling_resistance_wet_reduction: f32,
    /// Normal speed below which a bounce turns into rolling.
    pub roll_threshold: f32,
    pub post_restitution: f32,
    /// Max slope of the surface field at unevenness 1 (rolling deflection).
    pub surface_slope_max: f32,
    /// Max tilt of the bounce normal at unevenness 1.
    pub bounce_tilt_max: f32,
    /// Bobble probability per metre rolled at unevenness 1.
    pub bobble_rate: f32,
    pub stop_speed: f32,
}

impl Default for BallConfig {
    fn default() -> Self {
        Self {
            gravity: 9.81,
            air_density: 1.2,
            drag_coefficient: 0.25,
            magnus_coeff: 0.0035,
            spin_decay_air: 0.25,
            spin_decay_ground: 3.0,
            restitution_soft: 0.45,
            restitution_hard: 0.72,
            restitution_wet_loss: 0.15,
            bounce_friction_dry: 0.6,
            bounce_friction_wet: 0.25,
            rolling_resistance_short: 0.05,
            rolling_resistance_long: 0.11,
            rolling_resistance_wet_reduction: 0.03,
            roll_threshold: 0.5,
            post_restitution: 0.6,
            surface_slope_max: 0.035,
            bounce_tilt_max: 0.12,
            bobble_rate: 0.06,
            stop_speed: 0.05,
        }
    }
}

/// Mapping from attributes (0–100) to physical capabilities. The ranges run from attribute 0 to 100 and are centred
/// on the calibrated 58-rated player (top speed 8.8 m/s, acceleration 8.6 m/s²); their width is the physical slope of
/// the game, set with the `layers` bench alongside the duel and execution slopes (docs/design.md, attribute slope).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub top_speed_min: f32,
    pub top_speed_max: f32,
    /// Initial acceleration from rest (m/s²); it falls linearly to 0 at top speed, so a sprint follows the
    /// mono-exponential model v = top·(1 − e^(−t·accel/top)) with accel ≈ F0 (N/kg). Amateur players: F0 8.1 N/kg,
    /// V0 9.0 m/s, 10 m in 2.06 s, 30 m in 4.50 s; professionals 30 m in 4.15 s (docs/design.md).
    pub accel_min: f32,
    pub accel_max: f32,
    pub decel_min: f32,
    pub decel_max: f32,
    /// Lateral (turning) acceleration limits.
    pub lateral_accel_min: f32,
    pub lateral_accel_max: f32,
    /// Traction lost on a fully wet pitch.
    pub wet_traction_loss: f32,
    pub reaction_min: f32,
    pub reaction_max: f32,
    /// How long before a pass is struck its preparation (plant foot, backswing, body shape) can be read: every player's
    /// reaction to a pass (any kick but a shot) ends this much sooner after release, not before it (IND-08, `reach::pass_reaction`).
    pub pass_cue_s: f32,
    pub kick_speed_min: f32,
    pub kick_speed_max: f32,
    /// Body turn rate with the ball at the feet (rad/s), by agility and dribbling.
    pub ball_turn_rate_min: f32,
    pub ball_turn_rate_max: f32,
    /// Horizontal distance within which a player can play the ball with the feet.
    pub control_radius: f32,
    /// Max ball height (bottom of ball) a player can control without heading (chest).
    pub control_height: f32,
    pub tackle_reach_min: f32,
    pub tackle_reach_max: f32,
    /// Seconds after a kick before the kicker may touch the ball again.
    pub retouch_cooldown: f32,
    /// Distance between dribbler and ball at which control is lost.
    pub dribble_leash: f32,
    /// A carrier running towards his ball can play it with his next step: it is playable (he may decide and pass)
    /// within `control_radius` + this × his speed towards it (s).
    pub run_reach_s: f32,
    /// Settling the ball after gaining control, or after a knock-on, the carrier closes any gap to it within this
    /// time (s) to play it.
    pub settle_catch_s: f32,
    /// After a controlling touch the player's next decision (and so his next kick) comes this long (s) later, plus
    /// `settle_composure_s` × (1 − composure): he has to get to the ball and set himself. StatsBomb: of passes made
    /// after a controlled reception, only ≈ 9 % come within 0.5 s of it.
    pub settle_decision_s: f32,
    pub settle_composure_s: f32,
    /// The controlling touch goes only where the player can turn his run within this time (s) with his lateral
    /// acceleration: a player at speed takes the ball in his stride.
    pub touch_follow_s: f32,
    /// Separation radius between players.
    pub body_radius: f32,
    pub gk_dive_reach_min: f32,
    pub gk_dive_reach_max: f32,
    pub gk_dive_speed: f32,
    /// Goalkeeper reaction to a shot (s) at reflexes 20 and 1.
    pub gk_reaction_min: f32,
    pub gk_reaction_max: f32,
    /// Horizontal reach (m) of a standing goalkeeper's hands.
    pub gk_standing_reach: f32,
    /// A shot passing within this (m) of the keeper meets his body (half his width plus the ball's radius).
    pub gk_body_radius: f32,
    /// Max ball height (bottom of ball) a goalkeeper can catch.
    pub gk_max_height: f32,
    /// Chance that a shot passing within `body_radius` of an outfield opponent hits him (the body is in the way;
    /// a ball further out still needs a foot, see `foot_contact`). The same for any kick reaching an opponent within
    /// his reaction time of being struck (a blocked pass or cross).
    pub shot_body_block_p: f32,
    /// Shot blocks with a stretched leg or a slide: an outfield opponent gets a foot to a shot passing within
    /// `shot_block_reach` of him (ball no higher than `shot_block_height`) with chance `shot_leg_block_p` (scaled by
    /// instinct). Calibrated to StatsBomb blocked shares by defenders in the shooting triangle.
    pub shot_block_reach: f32,
    pub shot_block_height: f32,
    pub shot_leg_block_p: f32,
    /// Chance that an outfield opponent gets a touch on a pass running within `control_radius` of him:
    /// `pass_touch_base` − `pass_touch_per_mps` · ball speed, clamped to 0.35–0.95, scaled by instinct
    /// (`duel::pass_touch_prob`). The pass evaluation uses the same chance.
    pub pass_touch_base: f32,
    pub pass_touch_per_mps: f32,
    /// Fatigue (M6, `player::fatigue`). Running above `fatigue_hi_speed` (5.5 m/s = 19.8 km/h, the high-intensity
    /// threshold of the tracking studies, sourced [BRADLEY09]) takes the short-term energy down; below it energy
    /// recovers towards the long-term reserve with time constant `fatigue_recovery_s`. The reserve falls with the
    /// work of every metre run, work ∝ (speed / `fatigue_hi_speed`)². Both drains scale with
    /// 1 + `fatigue_stamina_range` · (0.5 − stamina). Energy takes top speed and acceleration down by
    /// `fatigue_speed_loss` · (1 − energy). Calibrated: second-half running per minute ≈ 7 % below the first
    /// [FIFA-WC22-RUN], high-intensity running in the last 15 minutes 20–45 % below the first 15 [BRADLEY09; MOHR03].
    pub fatigue_hi_speed: f32,
    pub fatigue_hi_drain: f32,
    pub fatigue_reserve_drain: f32,
    pub fatigue_recovery_s: f32,
    pub fatigue_stamina_range: f32,
    pub fatigue_speed_loss: f32,
    /// Confidence (M6, `player::confidence`): the step each event gives the player concerned, damped by
    /// 1 − `conf_composure_damp` · composure. A pass completed / lost (intercepted, won by the opponents or out of
    /// play); a duel won / lost (tackle, take-on, interception, aerial); a miscontrol; a shot on target (saved or in);
    /// a goal (the scorer, on top); a save (the keeper); a goal conceded (every player of the team; the keeper twice).
    /// A goal scored gives the team nothing: it does not change passing, conceding lowers it for ≈ 3–9 minutes
    /// [HORRILLO23], and goals show no self-affirmative effect [HEUER10; GAURIOT18]. Confidence returns to the
    /// pre-match level with time constant `conf_recovery_s` (sourced from the 3–9 minutes), below it faster by
    /// 1 + `conf_lift` · leadership of the strongest team-mate within `conf_lift_radius` (m). Steps calibrated small:
    /// pass completion within 10 minutes of conceding a little below the team's own, goal rate after scoring ≈ the
    /// team's own.
    pub conf_pass_completed: f32,
    pub conf_pass_lost: f32,
    pub conf_duel_won: f32,
    pub conf_duel_lost: f32,
    pub conf_miscontrol: f32,
    pub conf_shot_on_target: f32,
    pub conf_goal: f32,
    pub conf_save: f32,
    pub conf_conceded: f32,
    pub conf_composure_damp: f32,
    pub conf_recovery_s: f32,
    pub conf_lift: f32,
    pub conf_lift_radius: f32,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            top_speed_min: 7.4,
            top_speed_max: 9.8,
            accel_min: 6.6,
            accel_max: 10.0,
            decel_min: 5.4,
            decel_max: 7.7,
            lateral_accel_min: 4.5,
            lateral_accel_max: 7.1,
            wet_traction_loss: 0.15,
            reaction_min: 0.18,
            reaction_max: 0.38,
            pass_cue_s: 0.2,
            kick_speed_min: 24.0,
            kick_speed_max: 32.0,
            ball_turn_rate_min: 3.5,
            ball_turn_rate_max: 6.5,
            control_radius: 0.75,
            control_height: 1.3,
            tackle_reach_min: 1.1,
            tackle_reach_max: 1.6,
            retouch_cooldown: 0.25,
            dribble_leash: 2.6,
            run_reach_s: 0.1,
            settle_catch_s: 0.4,
            settle_decision_s: 0.35,
            settle_composure_s: 0.25,
            touch_follow_s: 0.5,
            body_radius: 0.45,
            gk_dive_reach_min: 1.9,
            gk_dive_reach_max: 2.7,
            gk_dive_speed: 5.5,
            gk_reaction_min: 0.12,
            gk_reaction_max: 0.32,
            gk_standing_reach: 1.2,
            gk_body_radius: 0.3,
            shot_body_block_p: 0.9,
            shot_block_reach: 1.4,
            shot_block_height: 0.7,
            shot_leg_block_p: 0.5,
            pass_touch_base: 1.25,
            pass_touch_per_mps: 0.04,
            gk_max_height: 2.45,
            fatigue_hi_speed: 5.5,
            fatigue_hi_drain: 0.01,
            fatigue_reserve_drain: 0.00004,
            fatigue_recovery_s: 60.0,
            fatigue_stamina_range: 0.8,
            fatigue_speed_loss: 0.1,
            conf_pass_completed: 0.005,
            conf_pass_lost: -0.015,
            conf_duel_won: 0.02,
            conf_duel_lost: -0.02,
            conf_miscontrol: -0.02,
            conf_shot_on_target: 0.02,
            conf_goal: 0.08,
            conf_save: 0.03,
            conf_conceded: -0.05,
            conf_composure_damp: 0.6,
            conf_recovery_s: 300.0,
            conf_lift: 1.0,
            conf_lift_radius: 25.0,
        }
    }
}

/// Layer-1 tactical parameters. Every field is named after its principle ID in
/// `docs/tactics/principles.md`, where its source and `sourced` / `calibrated` status are documented.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TacticsConfig {
    pub ph01_attacking_transition_s: f32,
    pub blk01_block_min_players: usize,
    pub blk01_stationary_speed: f32,
    pub blk01_organised_window_s: f32,
    pub blk01_organised_min_s: f32,
    pub blk01_min_behind_ball: usize,
    pub def02_cover_distance: f32,
    pub def02_cover_angle_deg: f32,
    pub def03_role_hysteresis_s: f32,
    /// DEF-03: an engaged first defender stays between ball and goal within this lateral distance (m) of the line.
    pub def03_engaged_half_width: f32,
    pub def04_zone_radius: f32,
    /// DEF-04: a marked opponent is released only this far outside the zone (handover hysteresis).
    pub def04_release_margin: f32,
    /// DEF-04 / IND-04: a marked opponent leaving the zone towards the own goal faster than this (m/s) is a runner. If
    /// no team-mate can take him over, his marker tracks him (`DefendingTactics::track_speed`).
    pub def04_track_speed: f32,
    /// DEF-04: a tracked runner is handed over once a team-mate is goal-side within this distance of him (m).
    pub def04_handover_distance: f32,
    /// BLK-03: a back-line defender's man more than `blk03_front_m` (m) in front of the line has dropped between the
    /// lines, and following him is stepping out. A defender follows him up to `blk03_step_out_m` in front of the line
    /// (0: never, he passes him on to the midfield; `DefendingTactics::step_out`). He may step out only while no other
    /// line defender is out, and only if a line team-mate whose slot is within `blk03_shift_m` of his own can shift
    /// across [FIFA-MB]. While he is out, the line on the side away from the ball shifts into his slot by
    /// `blk03_shuffle` of the gap to its nearest player (`DefendingTactics::shuffle`).
    pub blk03_front_m: f32,
    pub blk03_step_out_m: f32,
    pub blk03_shift_m: f32,
    pub blk03_shuffle: f32,
    pub def05_length_high: f32,
    pub def05_length_mid: f32,
    pub def05_length_low: f32,
    pub def05_width: f32,
    /// DEF-05: maximum speed (m/s) at which the block's line, front, centre and width follow their targets.
    pub def05_shift_speed: f32,
    pub def06_shift_factor: f32,
    pub blk02_pressure_radius: f32,
    pub blk02_gap_pressured: f32,
    pub blk02_gap_unpressured: f32,
    /// Near goal the line drops at most this share of the ball's distance from the goal line.
    pub blk02_max_drop_share: f32,
    /// BLK-02: the line leader changes his call (step up or drop) only once the new reading has held this long (s); a
    /// back pass or a clearance he calls at once.
    pub blk02_call_hold_s: f32,
    pub blk04_min_flight_s: f32,
    pub blk05_line_high: f32,
    pub blk05_line_mid: f32,
    pub blk05_line_low: f32,
    pub blk05_press_zone_high: f32,
    pub blk05_press_zone_mid: f32,
    pub blk05_press_zone_low: f32,
    pub prs01_slow_pass_speed: f32,
    pub prs01_touchline_zone: f32,
    pub prs01_poor_touch_quality: f32,
    pub prs01_trigger_duration_s: f32,
    /// PRS-01: a trigger lets the first defender press up to this far beyond the press zone (BLK-05).
    pub prs01_zone_extension: f32,
    /// PRS-01: a carrier standing closed to play (facing his own goal) with the first defender engaged goal-side of
    /// him is doubled [BDFL-PETER]: the team-mate on the side he faces who can reach him within `prs01_double_time_s`
    /// (s; 0: never) closes in to `prs01_double_distance` (m) in front of him and goes for the ball.
    pub prs01_double_time_s: f32,
    pub prs01_double_distance: f32,
    pub prs02_arrival_slack_s: f32,
    /// PRS-05: in a press with the carrier in the touchline zone (`prs01_touchline_zone`), the covering defender closes
    /// him from the inside at `prs05_trap_distance` (m), so the touchline, the first defender and he trap him
    /// ("numerical advantage around the ball" [FIFA-PR]). `prs05_trap` is how far he goes from his DEF-02 cover point
    /// to that trap point (1: all the way, 0: no trap).
    pub prs05_trap_distance: f32,
    pub prs05_trap: f32,
    pub prs03_decel_distance: f32,
    pub prs03_curve_offset: f32,
    pub prs04_lane_weight: f32,
    pub prs04_lane_radius: f32,
    pub prs07_delay_distance: f32,
    /// IND-05: a first defender is outnumbered when no team-mate is within `ind05_cover_m` (m) of the carrier and an
    /// unmarked attacker is within `ind05_support_m` (m) of him.
    pub ind05_cover_m: f32,
    pub ind05_support_m: f32,
    /// PRS-07: without a press the first defender steps at most this far beyond the block front.
    pub prs07_front_step: f32,
    pub ind01_contain_distance: f32,
    /// IND-01: within this distance (m) of the carrier the first defender mirrors him at his reaction time.
    pub ind01_track_distance: f32,
    pub ind02_show_offset: f32,
    pub ind03_tackle_min_p: f32,
    /// IND-03: a carrier who will be goal-side of the defender within this time (s) is challenged whatever the odds.
    pub ind03_last_ditch_s: f32,
    pub ind04_distance_per_m: f32,
    pub ind04_min: f32,
    pub ind04_max: f32,
    pub ind06_goal_half_width: f32,
    /// IND-07: against an opponent's high ball the first defender goes for it if he can be at the receiver's reception
    /// point no later than this (s) after the receiver: the receiver still has to bring a dropping ball down.
    pub ind07_challenge_slack_s: f32,
    /// BOX-01/02: the carrier is in shooting range within this distance (m) of the own goal centre.
    pub box01_range: f32,
    /// BOX-01: contain distance (m) of the first defender in shooting range ("getting tight").
    pub box01_contain_distance: f32,
    /// BOX-02: the cover stands on the shooting line this far (m) from the ball.
    pub box02_block_distance: f32,
    /// BOX-03: a teammate goal-side of the carrier within this distance (m) covers the first defender.
    pub box03_cover_radius: f32,
    /// BOX-03: tackle threshold of a covered first defender in shooting range (instead of `ind03_tackle_min_p`).
    pub box03_tackle_min_p: f32,
    /// BOX-04: the block starts to narrow when the ball is within this distance (m) of the own goal centre, and is
    /// fully narrowed at `box01_range`.
    pub box04_far: f32,
    /// CRS-01: with the ball in a crossing position (wide in the own third) the back line is at most this far (m) from
    /// the goal line, covering the penalty-spot area [FIFA-GK] (calibrated: defenders in the box at the release of a
    /// cross by its depth, StatsBomb 360).
    pub crs01_line_depth: f32,
    /// BOX-04: largest gap (m) between neighbouring back-line players once fully narrowed.
    pub box04_max_gap: f32,
    /// BOX-04: ball-side shift of the narrowed block (replaces `def06_shift_factor` near goal).
    pub box04_shift_factor: f32,
    /// BOX-04: distance (m) of the midfield line in front of the back line once fully narrowed.
    pub box04_mid_distance: f32,
    /// BOX-04: least urgency of a back-line player dropping to his line with the ball within `box04_far`.
    pub box04_drop_urgency: f32,
    pub trn01_min_criteria: u32,
    pub trn01_near_radius: f32,
    pub trn02_window_s: f32,
    pub trn03_squeeze_factor: f32,
    /// TRN-04 tactical foul: a player beaten within `trn04_window_s` of his team losing the ball, in the last
    /// `trn04_zone_m` of the pitch towards the opponents' goal, stops the counter with a foul with probability
    /// `trn04_foul_p` (instead of `RulesConfig::foul_beaten`), × (1 ± `foul_aggression`). Per team:
    /// `DefendingTactics::tactical_foul`.
    pub trn04_window_s: f32,
    pub trn04_zone_m: f32,
    pub trn04_foul_p: f32,
    pub gk02_smother_distance: f32,
    pub gk03_line_gap: f32,
    /// GK-04: the keeper comes for an opponent's pass into his area only if he is there this long (s) before its
    /// receiver, less his aerial command ("Command the space" [FIFA-GK]); otherwise he holds his position.
    pub gk04_claim_margin_s: f32,
    pub gk03_min_depth: f32,
    pub gk03_max_depth: f32,
    pub gk03_shot_range: f32,
    /// GK-01 angle narrowing within shooting range (`analysis::gk01_depth`): depth off the line with the ball at the
    /// penalty spot, per metre further out, at most `gk01_max_depth` and `gk01_depth_frac` of the way to the ball.
    pub gk01_narrow_depth: f32,
    pub gk01_depth_per_m: f32,
    pub gk01_max_depth: f32,
    pub gk01_depth_frac: f32,
    pub att01_shot_bonus: f32,
    pub att01_forward_pass_bonus: f32,
    pub att01_forward_dribble_bonus: f32,
    pub att02_risk_weight_own_third: f32,
    pub att02_risk_weight_middle_third: f32,
    pub att02_risk_weight_final_third: f32,
    pub att03_comfortable_p: f32,
    pub att03_forward_bonus_per_m: f32,
    pub att04_bind_bonus: f32,
    pub att06_pressure_time_s: f32,
    /// A first-time action is re-decided on the spot when he meets the ball farther than this (m) from where he
    /// planned it: 43 % of first-time shots were struck 8 m or more from the point their xG was priced at.
    pub att06_replan_m: f32,
    pub att07_short_max: f32,
    pub att07_long_min: f32,
    pub att08_min_distance: f32,
    pub att08_max_distance: f32,
    /// ATT-09 (sourced: "at least 3 options"): the check threshold of test 7; the count itself emerges from the
    /// offers' marginal value.
    pub att09_min_options: usize,
    /// ATT-10: the angle within which two supporting players count as collinear with the carrier (test threshold).
    pub att10_collinear_deg: f32,
    pub att11_onside_margin: f32,
    pub att11_touchline_margin: f32,
    /// ATT-11: height of the in-possession back line (m from the own goal line) with the ball at the centre of the
    /// own, middle and final third; interpolated between them (FIFA WC2022 means of the build-up, progression and
    /// final-third phases).
    pub att11_line: [f32; 3],
    /// ATT-11: in-possession team width (m, outfield extent) at the same three ball positions.
    pub att11_width: [f32; 3],
    pub att12_central_half_width: f32,
    pub att12_inside_half_width: f32,
    /// ATT-14: half-angle of a defender's cover shadow (test threshold; offers in it lose their race on the lane).
    pub att14_shadow_deg: f32,
    pub att15_min_space: f32,
    pub att17_line_break_bonus: f32,
    pub att17_last_line_factor: f32,
    pub att18_run_depth: f32,
    pub att18_min_time_on_ball_s: f32,
    /// ATT-26: a player whose anchor is within this (m) of the front of the in-possession shape is in the front line
    /// and chooses how to offer: pin the line, check short, or drop between the lines.
    pub att26_front_m: f32,
    /// ATT-26: up to this far (m) short of the onside point he checks short; beyond, he drops between the lines.
    pub att26_check_depth_m: f32,
    /// FT-03: the box-run spots, each (m from the goal line, m towards the ball's side of the goal centre): near post,
    /// far post, penalty spot, edge of the box [FIFA-ES; FIFA-TSG].
    pub ft03_spots: [[f32; 2]; 4],
    pub att21_density_ratio: f32,
    pub att21_switch_bonus: f32,
    /// ATT-21: pressure on the carrier (`analysis::pressure_at`) from which a switch counts as exploiting pressure
    /// he has attracted.
    pub att21_min_pressure: f32,
    /// ATT-24: a free receiver takes the pass where it was aimed if he gets there this long (s) before the ball.
    pub att24_set_margin_s: f32,
    /// ATT-24: how far (m) a receiver under pressure comes towards the passer while the pass is being set up.
    pub att24_meet_m: f32,
    /// ATT-25: share of a turnover expected when the carrier (after a dribble or while holding) is pressed in the
    /// ATT-06 sense: the value of that possession drops by this × pressed × (its value + the turnover cost there).
    pub att25_pressed_loss: f32,
    /// ATT-25: the same share for the receiver of a pass (`pressed_cal` example: the extra loss of pressed receivers).
    pub att25_receiver_loss: f32,
    /// ATT-25: share of a turnover expected for any receiver, pressed or not (closed down while he holds it):
    /// StatsBomb free receivers lose the ball before their next pass or shot 4.9 % of the time, the engine's 4–6 %
    /// (`pressed_cal`). Left at 0: pricing it (with `att25_receiver_loss` at the engine's own 0.25) made carriers
    /// pass less and carry more, and the losses rose with the tackles (docs/design.md, "One-step valuation").
    pub att25_receiver_base_loss: f32,
    pub bu01_gk_support_depth: f32,
    pub tr01_forward_bonus: f32,
    pub tr01_backward_penalty: f32,
    pub tr04_numbers_margin: i32,
    pub tr05_min_players: usize,
    pub tr05_max_players: usize,
    pub tr05_gap: f32,
    /// TR-05 (individual brains): how much later (in formation depth, 0..1) a wide player on the ball's side claims
    /// rest defence: he is the one who can join the attack, the far one tucks in ("2+3 or 3+2" [UEFA-TO]).
    pub tr05_ball_side_release: f32,
    /// TR-05: how much later a player instructed to go forward (`forward_runs` + `overlap`, times adherence) claims
    /// rest defence, and how much earlier one told to stay back.
    pub tr05_instruction_shift: f32,
    /// SP-01 defending corners: zonal players in the goal area (hybrid marking; the others mark player to player), an
    /// edge-of-the-box player, outfield players left up for the counter, the keeper's distance from his line (m).
    pub sp01_zonal: u8,
    pub sp01_edge: bool,
    pub sp01_counter: u8,
    pub sp01_gk_m: f32,
    /// SP-01..04: an organised set piece is taken once everybody is in position, after at least `sp01_setup_min_s`
    /// and at most `sp01_setup_max_s`.
    pub sp01_setup_min_s: f32,
    pub sp01_setup_max_s: f32,
    /// SP-02 attacking corners: attackers in the box; a short option near the taker.
    pub sp02_attackers: u8,
    pub sp02_short: bool,
    /// SP-02 delivery mix: the share of corners played short (when there is a short option), and the weights of the
    /// first post, the centre and the second post for the others.
    pub sp02_short_share: f32,
    pub sp02_delivery: [f32; 3],
    /// SP-03 free kicks: a direct free kick within `sp03_range_m` of goal is defended with a wall of up to
    /// `sp03_wall_max` (`set_piece::wall_size`) and the keeper `sp03_gk_shift_m` towards the far post; otherwise with
    /// a line `sp03_line_m` from the goal line and the keeper `sp03_gk_m` off his.
    pub sp03_range_m: f32,
    pub sp03_wall_max: u8,
    pub sp03_gk_shift_m: f32,
    pub sp03_line_m: f32,
    pub sp03_gk_m: f32,
    /// SP-03: a direct free-kick shot is curled over the wall at `sp03_speed` × the taker's kick speed to `sp03_height`
    /// m inside the far post; its xG ignores the wall's blocking, × `sp03_xg`.
    pub sp03_speed: f32,
    pub sp03_height: f32,
    pub sp03_xg: f32,
    /// SP-04 penalty kicks: the kicker's side (natural side, middle, other side by these shares) …
    pub sp04_natural: f32,
    pub sp04_middle: f32,
    /// … the keeper stays or dives (to the kicker's natural side by `sp04_keeper_natural` of his dives); staying, he
    /// covers a kick within `sp04_centre_m` of the middle. Players wait `sp04_edge_m` outside the area.
    pub sp04_keeper_stays: f32,
    pub sp04_keeper_natural: f32,
    pub sp04_centre_m: f32,
    pub sp04_edge_m: f32,
    /// SP-04: the kicker aims `sp04_aim_m` from the middle of the goal, `sp04_height` m high, at `sp04_speed` × his
    /// kick speed.
    pub sp04_aim_m: f32,
    pub sp04_height: f32,
    pub sp04_speed: f32,
    /// SP-04: a keeper who guessed the side saves the kick `sp04_save_right` × (0.5 + reflexes) of the time (ACAD-PK02:
    /// 36–56 % of kicks are not scored when he guesses right, misses included).
    pub sp04_save_right: f32,
    /// SP-04: a penalty kick's xG (OPTA-PK01: "historically about 78% of them are scored").
    pub sp04_xg: f32,
    /// SP-06 throw-ins: thrown with both hands from over the head (Law 15), released at `sp06_height` m at
    /// `sp06_angle` rad, at up to `sp06_speed_min` + (`sp06_speed_max` − `sp06_speed_min`) × strength m/s.
    pub sp06_height: f32,
    pub sp06_angle: f32,
    pub sp06_speed_min: f32,
    pub sp06_speed_max: f32,
}

impl Default for TacticsConfig {
    fn default() -> Self {
        Self {
            ph01_attacking_transition_s: 6.0,
            blk01_block_min_players: 8,
            blk01_stationary_speed: 2.78,
            blk01_organised_window_s: 2.0,
            blk01_organised_min_s: 1.6,
            blk01_min_behind_ball: 4,
            def02_cover_distance: 6.0,
            def02_cover_angle_deg: 30.0,
            def03_role_hysteresis_s: 0.3,
            def03_engaged_half_width: 2.0,
            def04_zone_radius: 12.0,
            def04_release_margin: 4.0,
            def04_track_speed: 4.0,
            def04_handover_distance: 3.0,
            blk03_front_m: 4.0,
            blk03_step_out_m: 12.0,
            blk03_shift_m: 16.0,
            blk03_shuffle: 0.5,
            def05_length_high: 30.0,
            def05_length_mid: 27.2,
            def05_length_low: 24.3,
            def05_width: 40.1,
            def05_shift_speed: 3.0,
            def06_shift_factor: 0.65,
            blk02_pressure_radius: 2.5,
            blk02_gap_pressured: 8.0,
            blk02_gap_unpressured: 16.0,
            blk02_max_drop_share: 0.5,
            blk02_call_hold_s: 0.4,
            blk04_min_flight_s: 0.8,
            blk05_line_high: 2.0,
            blk05_line_mid: -15.0,
            blk05_line_low: -35.0,
            blk05_press_zone_high: 52.5,
            blk05_press_zone_mid: 15.0,
            blk05_press_zone_low: -17.5,
            prs01_slow_pass_speed: 8.0,
            prs01_touchline_zone: 27.0,
            prs01_poor_touch_quality: 0.5,
            prs01_trigger_duration_s: 2.0,
            prs01_zone_extension: 15.0,
            prs01_double_time_s: 2.0,
            prs01_double_distance: 1.0,
            prs02_arrival_slack_s: 0.4,
            prs05_trap_distance: 4.0,
            prs05_trap: 1.0,
            prs03_decel_distance: 4.0,
            prs03_curve_offset: 2.0,
            prs04_lane_weight: 1.0,
            prs04_lane_radius: 3.0,
            prs07_delay_distance: 4.0,
            ind05_cover_m: 10.0,
            ind05_support_m: 15.0,
            prs07_front_step: 3.0,
            ind01_contain_distance: 1.8,
            ind01_track_distance: 5.0,
            ind02_show_offset: 0.6,
            ind03_tackle_min_p: 0.65,
            ind03_last_ditch_s: 0.3,
            ind04_distance_per_m: 0.08,
            ind04_min: 1.0,
            ind04_max: 5.0,
            ind06_goal_half_width: 3.66,
            ind07_challenge_slack_s: 1.0,
            box01_range: 22.0,
            box01_contain_distance: 1.2,
            box02_block_distance: 3.5,
            box03_cover_radius: 5.0,
            box03_tackle_min_p: 0.45,
            box04_far: 45.0,
            crs01_line_depth: 14.0,
            box04_max_gap: 6.0,
            box04_shift_factor: 0.4,
            box04_mid_distance: 6.0,
            box04_drop_urgency: 0.7,
            trn01_min_criteria: 3,
            trn01_near_radius: 15.0,
            trn02_window_s: 5.0,
            trn03_squeeze_factor: 0.85,
            trn04_window_s: 5.0,
            trn04_zone_m: 60.0,
            trn04_foul_p: 0.22,
            gk02_smother_distance: 3.0,
            gk03_line_gap: 20.4,
            gk04_claim_margin_s: 0.6,
            gk03_min_depth: 1.0,
            gk03_max_depth: 18.0,
            gk03_shot_range: 40.0,
            gk01_narrow_depth: 1.8,
            gk01_depth_per_m: 0.12,
            gk01_max_depth: 2.5,
            gk01_depth_frac: 0.3,
            att01_shot_bonus: 0.1,
            att01_forward_pass_bonus: 0.1,
            att01_forward_dribble_bonus: 0.05,
            att02_risk_weight_own_third: 1.4,
            att02_risk_weight_middle_third: 1.0,
            att02_risk_weight_final_third: 0.6,
            att03_comfortable_p: 0.8,
            att03_forward_bonus_per_m: 0.005,
            att04_bind_bonus: 0.15,
            att06_pressure_time_s: 0.6,
            att06_replan_m: 2.0,
            att07_short_max: 10.0,
            att07_long_min: 30.0,
            att08_min_distance: 6.0,
            att08_max_distance: 25.0,
            att09_min_options: 3,
            att10_collinear_deg: 12.0,
            att11_onside_margin: 0.7,
            att11_touchline_margin: 2.5,
            att11_line: [19.6, 40.3, 53.3],
            att11_width: [54.1, 55.3, 44.0],
            att12_central_half_width: 9.16,
            att12_inside_half_width: 20.16,
            att14_shadow_deg: 10.0,
            att15_min_space: 3.0,
            att17_line_break_bonus: 0.12,
            att17_last_line_factor: 2.0,
            att18_run_depth: 12.0,
            att18_min_time_on_ball_s: 0.8,
            att26_front_m: 5.0,
            att26_check_depth_m: 8.0,
            ft03_spots: [[7.0, 3.0], [7.0, -4.0], [11.0, 0.0], [17.0, 4.0]],
            att21_density_ratio: 1.5,
            att21_switch_bonus: 0.2,
            att21_min_pressure: 0.5,
            att24_set_margin_s: 0.3,
            att24_meet_m: 2.0,
            att25_pressed_loss: 0.3,
            att25_receiver_loss: 0.18,
            att25_receiver_base_loss: 0.0,
            bu01_gk_support_depth: 8.0,
            tr01_forward_bonus: 0.25,
            tr01_backward_penalty: 0.25,
            tr04_numbers_margin: 1,
            tr05_min_players: 4,
            tr05_max_players: 5,
            tr05_gap: 3.0,
            tr05_ball_side_release: 0.3,
            tr05_instruction_shift: 0.5,
            sp01_zonal: 3,
            sp01_edge: true,
            sp01_counter: 0,
            sp01_gk_m: 1.0,
            sp01_setup_min_s: 3.0,
            sp01_setup_max_s: 15.0,
            sp02_attackers: 5,
            sp02_short: true,
            sp02_short_share: 0.19,
            sp02_delivery: [0.423, 0.238, 0.205],
            sp03_range_m: 32.0,
            sp03_wall_max: 5,
            sp03_gk_shift_m: 1.0,
            sp03_line_m: 16.0,
            sp03_gk_m: 3.7,
            sp03_speed: 0.68,
            sp03_height: 1.9,
            sp03_xg: 0.67,
            sp04_natural: 0.449,
            sp04_middle: 0.172,
            sp04_keeper_stays: 0.063,
            sp04_keeper_natural: 0.58,
            sp04_centre_m: 1.5,
            sp04_edge_m: 1.0,
            sp04_aim_m: 2.2,
            sp04_height: 0.5,
            sp04_speed: 0.8,
            sp04_save_right: 0.3,
            sp04_xg: 0.78,
            sp06_height: 2.2,
            sp06_angle: 0.56,
            sp06_speed_min: 12.0,
            sp06_speed_max: 19.0,
        }
    }
}

/// Possession value on a grid of `cell_m` squares over the team frame (M8): x from the own goal line (`nx` cells),
/// |y| from the centre line (`ny` cells, both wings folded onto one), row-major by x. `analysis::ValueGrid` methods
/// look it up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueGrid {
    pub cell_m: f32,
    pub nx: usize,
    pub ny: usize,
    pub v: Vec<f32>,
}

/// Engine-level AI settings (not tactical principles): decision cadence, value model, positioning sampling.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Movement model (section 9): a player holds his movement between triggers and re-checks it every this many
    /// seconds; while he holds it he follows its path at his reaction time.
    pub movement_check_s: f32,
    /// Look-ahead (docs/player-brain.md section 8.4): a perfect reader (instinct and intelligence 100) projects this many
    /// seconds ahead, a reader at 0 not at all. A team-mate's pass in flight is read as the next carrier's ball once it
    /// arrives within his horizon, and the defenders' responses to a movement weigh by the same share (space value).
    pub lookahead_horizon_s: f32,
    /// Show (section 6): he shows only where he arrives within this many seconds, by the carrier's next decision.
    pub show_reach_s: f32,
    /// Offer value (section 8.3): σ-like scale of how much the carrier's options change in value before he plays the
    /// ball (a lane closes, a receiver is pressed, the pressure on him grows), as a share of the local value scale. It
    /// joins his decision temperature (`offer::temperature`), so a second and a third option are worth offering.
    pub offer_uncertainty: f32,
    /// Offer value (section 8.3): two offers closer than this (m) are one option for the carrier (whatever closes one
    /// closes the other), so a second run into the same space adds nothing.
    pub offer_overlap_m: f32,
    /// Reading team-mates (section 7): player i reads a team-mate's movement or claim that differs from what the
    /// concept prescribes after `reading_delay_max_s` × (1 − u), with understanding u = intelligence × f(practice) ×
    /// f(partners) × f(1 − instruction load), f(x) = floor + (1 − floor)·x (`ai::reading`). Until then he believes
    /// the concept's. A perfect reader (u = 1) reads at once.
    pub reading_delay_max_s: f32,
    /// Floor of f(practice): the mean of the formation and concept practice (practice weighs less than partners and
    /// the role, section 15).
    pub reading_practice_floor: f32,
    /// Floor of f(partners): how well the two know each other.
    pub reading_partner_floor: f32,
    /// Floor of f(1 − load): his instruction load (a role he is new to, instructions the team has not practised).
    pub reading_load_floor: f32,
    /// Reading the passer (section 7): the intended receiver of a pass anticipates it as far as he knows the passer
    /// and the team's play, F = f(practice) × f(partners) × f(1 − load) (u without intelligence: his general reading
    /// of play is in his reaction). He starts to take it `reading_receive_delay_max_s` × (1 − F) after it becomes
    /// legible (the set-up or the strike), until then keeping his movement. The passer prices the delay
    /// (`analysis::evaluate_pass`).
    pub reading_receive_delay_max_s: f32,
    /// Effort (section 8.5): cost per metre to the movement's point at a jog, as a share of the local value scale
    /// (possession value at the ball + base), × the movement's speed class (walk 0.25, jog 0.5, run 1, sprint 2) and
    /// ÷ (0.5 + work rate), × (1 + `effort_fatigue` · (1 − energy)): a tired player makes fewer of the optional
    /// efforts (M6; calibrated: high-intensity running in the last 15 minutes 20–45 % below the first 15, with longer
    /// recoveries between the bouts late on [BRADLEY09; MOHR03]).
    pub effort_per_m: f32,
    pub effort_fatigue: f32,
    /// Hysteresis: the movement he holds is kept unless another is better by this share of the local value scale.
    pub movement_switch_margin: f32,
    /// Layer 1: the role's movement prior (0..1, `data/roles`) adds this share of the local value scale.
    pub role_prior_weight: f32,
    /// Layer 2: a player instruction at ±1 (`forward_runs`, `come_short`, `overlap`, `invert`) adds ± this share of the local value scale
    /// to the movements it asks for, times his adherence (`team::adherence`).
    pub movement_instruction_weight: f32,
    /// Layer 3: σ of the noise on a movement's value, as a share of the local value scale, for a player of reliability
    /// 0 (none at 1).
    pub movement_noise: f32,
    /// `P_play` of a ball in behind (a run's offer) when the carrier is closed (pressed, or facing his own goal): he
    /// can hardly play it [FA-FWD].
    pub offer_closed_play: f32,
    /// Interval of the player brains' periodic update; they also update at once when the ball is struck, saved or lost.
    pub team_tick_s: f32,
    pub offball_interval_min: f32,
    pub offball_interval_max: f32,
    /// Concentration (M6, `deviation::lapse_rate`): at each off-ball re-read, a lapse with chance rate × the interval
    /// to the next; he then does not re-read for `lapse_s`. Rate `lapse_rate` per minute × (1 − `lapse_composure` ·
    /// composure) × (1 + `lapse_fatigue` · (1 − energy)). Mental fatigue slows soccer decisions and skills [SMITH16;
    /// BADIN16]; calibrated: goals by period rise over the match without stoppage time and game state (second half ≈
    /// 1.1–1.2 × the first).
    pub lapse_rate: f32,
    pub lapse_composure: f32,
    pub lapse_fatigue: f32,
    pub lapse_s: f32,
    /// Calls (M6, docs/player-brain.md section 7, `reading::call_rate`): `call_rate` calls per minute at leadership
    /// 100, confidence 0.5 and no load, up to `call_max` in hand. A player voices an exclusive claim or staying back,
    /// the line leader a change of his call, the chaser "mine"; a team-mate within `call_range` (m) of a receiver who
    /// does not see the pressure on him shouts "man on" (his picture of the opponents is refreshed). Team-mates hear a
    /// call at their next update instead of after their reading delay. Calibrated: no published rate.
    pub call_rate: f32,
    pub call_max: f32,
    pub call_range: f32,
    pub onball_interval: f32,
    /// Hold / shield (`on_ball`): share of the ball lost before the next decision (`onball_interval` away) at full
    /// pressure, before shielding (strength, balance, dribbling) takes off up to 60 % of it. Calibrated against what
    /// follows a hold (`hold_cal`: 97.5 % still have the ball at the next decision under full pressure; 0.6 priced it at
    /// 72 %); the loss over the rest of the spell is ATT-25's (`pressed_loss`), so a larger value counted it twice.
    pub hold_loss: f32,
    /// How long (s) a player's misjudgement of his on-ball options persists before it is drawn afresh.
    pub decision_noise_hold_s: f32,
    /// … and while the pressure on him stays within one of this many equal bands (0: pressure ignored).
    pub decision_noise_pressure_bands: u32,
    /// Weight of staying near the formation anchor in positioning.
    pub shape_weight: f32,
    /// Distance scale (m) of the shape term.
    pub shape_scale: f32,
    /// Ring radii (m) and directions used to sample positioning candidates.
    pub candidate_radii: Vec<f32>,
    pub candidate_directions: usize,
    /// Softness (s) of interception races.
    pub race_sigma: f32,
    /// How long (s) after a lofted pass lands its first-arrival race is followed (`analysis::evaluate_pass`): the
    /// bounces in which the receiver or an opponent can still get to it, or it runs out of play.
    /// Share of contested headers after which the heading side next controls the ball: after a won duel and after a
    /// lost one (the knock-down is a loose ball either way; calibrated with `pass_cal`).
    pub aerial_won_keep: f32,
    pub aerial_lost_keep: f32,
    /// Worth of a knock-down his side gets after a header, as a share of the ball received at the target.
    pub aerial_value_factor: f32,
    pub lofted_race_tail_s: f32,
    /// Launch angle (rad) of a lofted pass (ATT-07). Calibrated on hang time: StatsBomb open-play high passes of
    /// 30–45 m reach the next event (receipt or contest) after a median 2.45 s (complete) / 2.56 s (failed); the
    /// engine's take 2.14 s at 0.5 rad, 2.40 s at 0.6, 2.52 s at 0.65, 2.62 s at 0.7 (`passes`).
    pub lofted_launch_angle: f32,
    /// Launch angle (rad) of a cross (FT-02), calibrated on hang time: StatsBomb 360 open-play high crosses take a
    /// median 0.87 s (0–15 m), 1.23 s (15–25 m) and 1.62 s (25–35 m); the solver's at 0.30 rad 0.90 / 1.19 / 1.49 s, at
    /// 0.35 rad 0.98 / 1.29 / 1.62 s (12 / 20 / 30 m).
    pub cross_launch_angle: f32,
    /// Coarser integration step used for planning (same step function as live physics).
    pub planning_dt: f32,
    /// Possession value model (≈ probability of scoring from possession at a location): `value_base` +
    /// `value_progress` × s^`value_power` × (`value_wide` + (1 − `value_wide`) × exp(−(y / `value_width`)²)) +
    /// `value_shot_share` × the unblocked xG there, s = metres from the own goal line / pitch length.
    pub value_base: f32,
    pub value_progress: f32,
    pub value_power: f32,
    pub value_wide: f32,
    pub value_width: f32,
    pub value_shot_share: f32,
    /// A possession value field learned by self-play (M8, `match_lab --learn-value`) in place of the parametric model
    /// above; `None` keeps the parametric model.
    pub value_grid: Option<ValueGrid>,
    /// Continuation value of keeping the ball (`on_ball`): a carrier who dribbles or holds can still play the best
    /// pass on offer afterwards, so its lead over the value of his location is added to the end point's value. It
    /// fades with the time until he can play it, as defenders close the lane; this is its half-life (s).
    pub continuation_halflife_s: f32,
    /// xG logistic model: logit = a + b·(goal angle) − c·distance − lateral·|offset from the centre line| (m), for a
    /// reference finisher's controlled strong-foot shot, keeper set, nobody in the way (fit to StatsBomb open data with
    /// first-time and weak-foot terms; first time is priced by `finish_factor`).
    pub xg_a: f32,
    pub xg_b: f32,
    pub xg_c: f32,
    pub xg_lateral: f32,
    /// Finishing in xG (`analysis::finish_factor`): exponent on the ratio of execution error scales, and the shooting
    /// skill of the reference finisher the logistic describes.
    pub xg_finish_exp: f32,
    pub xg_ref_skill: f32,
    pub xg_blocker_factor: f32,
    pub xg_pressure_factor: f32,
    /// xG of a shot at an empty goal from a wide-enough angle: `xg_open_goal · min(goal angle / xg_open_angle,
    /// 1)^xg_open_exp`, so an open goal from 25 m is far from a sure thing.
    pub xg_open_goal: f32,
    pub xg_open_angle: f32,
    pub xg_open_exp: f32,
    /// Mean speed of a shot over its flight, as a fraction of the shooter's maximum kick speed (keeper race in xG).
    pub xg_shot_speed_factor: f32,
    /// Softness (s) of the keeper's race to the shot's path in xG.
    pub xg_keeper_race_sigma: f32,
    pub max_shot_distance: f32,
    pub pass_max_distance: f32,
    /// Ground pass weight: speed (m/s) at the receiver, `pass_arrival_base` + `pass_arrival_per_m` per metre of pass,
    /// within `pass_arrival_min`..`pass_arrival_max`. Calibrated on StatsBomb's release-to-reception `duration` of
    /// completed ground passes (median 0.90 / 1.25 / 1.70 / 2.24 s at 0–10 / 10–20 / 20–30 / 30–45 m).
    pub pass_arrival_base: f32,
    pub pass_arrival_per_m: f32,
    pub pass_arrival_min: f32,
    pub pass_arrival_max: f32,
    /// Softmax temperature for a player with 0/100 decisions, and for 100/100 decisions at full confidence and energy
    /// (0 = he always takes the layer-1 optimum), as a fraction of the local value scale (possession value at the ball +
    /// `value_base`).
    pub temperature_worst: f32,
    pub temperature_best: f32,
    /// Exponent on the decision skill in the temperature curve: temperature = worst + (best − worst) · skill^exp. Above
    /// 1 the curve is concave (flat through the middle of the scale, steep near 100); 0.23 / 2.0 keep the calibrated
    /// 58-rated player at the temperature he had with 0.35 / 1.0 (attribute-slope round, 2026-10-29).
    pub temperature_exp: f32,
    /// Vision cone half-angle (deg) at intelligence 0 and 100.
    pub vision_half_angle_min: f32,
    pub vision_half_angle_max: f32,
    /// Duration of the kicking motion / follow-through.
    pub kick_windup_s: f32,
    pub gk_hold_s: f32,
    /// Layer-2 decision bias of a fully applied instruction at strength 1 (fraction of the local value scale).
    pub instruction_bias: f32,
    /// Layer 3: how much of his intelligence (off the ball) or instinct (on the ball) a player keeps at zero
    /// confidence, zero energy and zero role familiarity (1 at full; linear between; `deviation::reliability`). Only
    /// the 100-rated player at full confidence, energy and familiarity is exactly on the optimum.
    pub reliability_confidence_floor: f32,
    pub reliability_energy_floor: f32,
    pub reliability_familiarity_floor: f32,
    /// Ornstein–Uhlenbeck mean-reversion rate (1/s) of layer-3 positional drift.
    pub drift_theta: f32,
    /// Positional drift σ (m) for the least tactically reliable player.
    pub drift_sigma_max: f32,
    /// Utility noise σ (score units) in positioning for the least tactically reliable player.
    pub position_noise_max: f32,
    /// Score cost per metre of moving away from the committed positioning point (decision inertia).
    pub commit_cost: f32,
    /// Positional play: corrections of the movement target smaller than this are ignored (m).
    pub settle_radius: f32,
    /// ATT-05 scanning (M6, `ai::scan`): scans per second from `scan_rate_min` (intelligence 0) to `scan_rate_max`
    /// (100), × `scan_pressed_share` with an opponent within 1 m, the full rate from 4 m. Sourced: 0.44 ± 0.30 scans/s
    /// in the 10 s before receiving, fewest with an opponent within 1 m [JORDET20]. Field of view
    /// `scan_view_half_angle` (deg) around the facing; outside it, an opponent is where the last scan saw him, carried
    /// on for at most `scan_extrapolate_s`. Settling after control × (1 − `scan_settle_gain`) with a scan within
    /// `scan_fresh_s` and no pressure, × (1 + `scan_settle_gain`) with none within `scan_stale_s`. Calibrated: pass
    /// completion rises with the scans before receiving (64 / 68 / 81 % for low / medium / high [JORDET13]).
    pub scan_rate_min: f32,
    pub scan_rate_max: f32,
    pub scan_pressed_share: f32,
    pub scan_view_half_angle: f32,
    pub scan_extrapolate_s: f32,
    /// LAW-01: a passer judges a team-mate's offside position against the line he perceives (ATT-05), with a
    /// judgement error of σ = `offside_judge_m` × (1.25 − 0.5 × intelligence) m.
    pub offside_judge_m: f32,
    pub scan_fresh_s: f32,
    pub scan_stale_s: f32,
    pub scan_settle_gain: f32,
    /// Positional play: distance from the anchor (m) at which a player moves at 85 % urgency; closer, he slows.
    pub position_full_speed_dist: f32,
    /// Pacing (M6, `positioning::pace_time`): the time (s) in which a player closes the gap to his target, by kind of
    /// movement: positional play (shape, support, rest defence, balance and the possession movements other than a
    /// check, show, overlap or box run), cover, marking (without a pass to his man). Pressing, tracking a runner,
    /// receiving, chasing and the efforts of the possession movements are not paced; a recovery run (IND-06) goes as
    /// fast as he needs to be goal-side before the ball gets there, at its speed towards goal but at least
    /// `pace_ball_speed` (m/s). Longer by
    /// `pace_fatigue` · (1 − energy), and by `pace_work_rate` · (0.5 − work rate). Calibrated: high-intensity running
    /// (> 19.8 km/h) ≈ 530–980 m per player and match in 30–45 efforts of ≈ 20 m [JU23], total distance ≈ 10.6–11.4 km
    /// [DISALVO07]. Per 90 minutes in play ≈ 13–13.6 km, not × 1.48: the in-play rate is about twice the out-of-play
    /// one [JEROME23], 10.94 km of which in 57.5 minutes in play [ALTMANN23].
    pub pace_position_s: f32,
    pub pace_cover_s: f32,
    pub pace_mark_s: f32,
    pub pace_ball_speed: f32,
    pub pace_fatigue: f32,
    pub pace_work_rate: f32,
    /// Pacing: the top speed (m/s) of a paced movement, a run below the high-intensity threshold
    /// (`PlayerConfig::fatigue_hi_speed`). Calibrated as the pace times.
    pub pace_run_speed: f32,
    /// Positional play: minimum urgency (walking pace).
    pub position_min_urgency: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            movement_check_s: 1.5,
            lookahead_horizon_s: 2.0,
            show_reach_s: 2.0,
            offer_uncertainty: 0.5,
            offer_overlap_m: 6.0,
            reading_delay_max_s: 1.5,
            reading_practice_floor: 0.7,
            reading_partner_floor: 0.2,
            reading_load_floor: 0.3,
            reading_receive_delay_max_s: 0.6,
            effort_per_m: 0.003,
            effort_fatigue: 3.0,
            movement_switch_margin: 0.05,
            role_prior_weight: 0.05,
            movement_instruction_weight: 1.0,
            movement_noise: 0.1,
            offer_closed_play: 0.15,
            team_tick_s: 0.2,
            offball_interval_min: 0.25,
            offball_interval_max: 0.6,
            lapse_rate: 1.0,
            lapse_composure: 0.7,
            lapse_fatigue: 3.0,
            lapse_s: 1.5,
            call_rate: 6.0,
            call_max: 2.0,
            call_range: 30.0,
            onball_interval: 0.3,
            hold_loss: 0.05,
            decision_noise_hold_s: 2.0,
            decision_noise_pressure_bands: 3,
            shape_weight: 1.0,
            shape_scale: 6.0,
            candidate_radii: vec![3.0, 6.5, 10.0],
            candidate_directions: 8,
            race_sigma: 0.18,
            aerial_won_keep: 0.6,
            aerial_lost_keep: 0.28,
            aerial_value_factor: 0.5,
            lofted_race_tail_s: 1.2,
            lofted_launch_angle: 0.65,
            cross_launch_angle: 0.33,
            planning_dt: 0.01,
            value_base: 0.004,
            value_progress: 0.06,
            value_power: 3.0,
            value_wide: 0.4,
            value_width: 22.0,
            value_shot_share: 0.55,
            value_grid: None,
            continuation_halflife_s: 1.0,
            xg_a: -0.18,
            xg_b: 0.97,
            xg_c: 0.116,
            xg_lateral: 0.065,
            xg_finish_exp: 0.65,
            xg_ref_skill: 0.65,
            xg_blocker_factor: 0.45,
            xg_pressure_factor: 0.35,
            xg_open_goal: 0.92,
            xg_open_angle: 0.6,
            xg_open_exp: 1.5,
            xg_shot_speed_factor: 0.75,
            xg_keeper_race_sigma: 0.15,
            max_shot_distance: 32.0,
            pass_max_distance: 50.0,
            pass_arrival_base: 4.0,
            pass_arrival_per_m: 0.38,
            pass_arrival_min: 6.0,
            pass_arrival_max: 12.0,
            temperature_worst: 0.23,
            temperature_exp: 2.0,
            temperature_best: 0.0,
            vision_half_angle_min: 80.0,
            vision_half_angle_max: 150.0,
            kick_windup_s: 0.18,
            gk_hold_s: 2.5,
            instruction_bias: 0.3,
            reliability_confidence_floor: 0.6,
            reliability_energy_floor: 0.6,
            reliability_familiarity_floor: 0.7,
            drift_theta: 0.5,
            drift_sigma_max: 2.0,
            position_noise_max: 0.4,
            commit_cost: 0.3,
            settle_radius: 2.0,
            scan_rate_min: 0.1,
            scan_rate_max: 0.8,
            scan_pressed_share: 0.4,
            scan_view_half_angle: 100.0,
            scan_extrapolate_s: 0.5,
            offside_judge_m: 2.8,
            scan_fresh_s: 1.5,
            scan_stale_s: 4.0,
            scan_settle_gain: 0.3,
            position_full_speed_dist: 20.0,
            pace_position_s: 4.0,
            pace_cover_s: 1.5,
            pace_mark_s: 2.0,
            pace_ball_speed: 2.5,
            pace_fatigue: 2.5,
            pace_work_rate: 0.6,
            pace_run_speed: 5.0,
            position_min_urgency: 0.08,
        }
    }
}

/// Layer-3 personality biases on decisions (fractions of the local value scale; see `ai::on_ball`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersonalityConfig {
    /// Bonus for self actions (dribble, shoot, hold) at full egotism.
    pub egotism_self_bonus: f32,
    /// The shot's self bonus at full egotism as a share of the shot's xG (egotists overrate their chance).
    pub egotism_shot_share: f32,
    /// Penalty on passes at full egotism; doubled for square/back passes and passes to better-placed teammates.
    pub egotism_pass_penalty: f32,
    pub flair_bonus: f32,
    /// Max reduction of the turnover weight from aggression.
    pub aggression_risk_reduction: f32,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            egotism_self_bonus: 0.25,
            egotism_shot_share: 0.5,
            egotism_pass_penalty: 0.15,
            flair_bonus: 0.12,
            aggression_risk_reduction: 0.3,
        }
    }
}

/// Execution error of kicks (layer 3 on technique).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionConfig {
    /// Base angular error (rad) for an average player in ideal conditions.
    pub angle_ground: f32,
    pub angle_lofted: f32,
    pub angle_shot: f32,
    pub speed_rel: f32,
    pub elevation: f32,
    /// Elevation error (rad) of shots: struck hard, often from a moving ball, so they rise or dip far more than passes.
    pub elevation_shot: f32,
    pub spin_rel: f32,
    pub first_time_factor: f32,
    /// Error multiplier at kicking skill 0 and at skill 1 (linear between; 0.90 at the calibrated 0.58). The ratio
    /// is the technical slope of the game: calibrated with the `layers` bench alongside the duel slopes.
    pub error_scale_worst: f32,
    pub error_scale_best: f32,
    /// Error scale of a kick with the weak foot at weak-foot ability 0 (1 at ability 1, linear in between).
    pub weak_foot_max_factor: f32,
    /// How far (deg) a foot can kick towards its own side for a soft kick (inside of the foot with the hips open, or
    /// the outside of the foot) …
    pub foot_outside_soft_deg: f32,
    /// … and for a kick at the kicker's maximum speed (the instep swings across the body from a diagonal run-up: the
    /// right foot strikes to the left); linear in between. Beyond it the body must turn further, or the other foot
    /// strikes (`execution::natural_foot`).
    pub foot_outside_hard_deg: f32,
    /// How much longer (s) a player with weak-foot ability 0 will take to set a pass up on his strong foot rather than
    /// play it with the weak one; scaled by `1 − weak-foot ability`.
    pub strong_foot_slack_s: f32,
    pub pressure_factor: f32,
    /// Chance of a shank at kick skill 0 (falling linearly to none at 1).
    pub shank_probability: f32,
    pub shank_factor: f32,
    /// Ground passes use short passing up to this length (m) …
    pub short_pass_full_m: f32,
    /// … blending into long passing, which they use completely from this length (ATT-07 puts the long category at
    /// 30 m; the blend avoids a cliff).
    pub long_pass_full_m: f32,
    /// Shots use finishing up to this distance (m) from goal …
    pub finishing_full_m: f32,
    /// … blending into long shots, which they use completely from this distance.
    pub long_shots_full_m: f32,
    /// Kick set-up: largest angle (deg) between body and kick for a soft kick (inside of the foot across the body) …
    pub setup_angle_soft_deg: f32,
    /// … and for a kick at the player's maximum kick speed (body square to the kick); linear in between.
    pub setup_angle_hard_deg: f32,
    /// Kick set-up: the kicker may still move away from the kick direction at up to this speed (m/s).
    pub setup_back_speed: f32,
    /// Kick set-up: the kick is struck regardless this long (s) after the estimated set-up time.
    pub setup_timeout_s: f32,
    /// Execution error grows by this fraction per radian the body is still turned beyond the set-up angle.
    pub body_angle_factor: f32,
    /// Shots that need a set-up keep exp(-t / this) of their xG (defenders close down and block meanwhile).
    pub setup_shot_decay_s: f32,
    /// ATT-05: a first-time receiver facing the incoming ball can open his body by this much (deg) towards the kick.
    pub first_time_open_deg: f32,
    /// Speed–accuracy of shots: the error scale is multiplied by (speed fraction / `shot_speed_ref`)^`shot_speed_exp`,
    /// so a placed shot (≈ 0.8 of the kicker's maximum) keeps the base error and a full-power one is less accurate.
    pub shot_speed_ref: f32,
    pub shot_speed_exp: f32,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            angle_ground: 0.035,
            angle_lofted: 0.045,
            angle_shot: 0.12,
            speed_rel: 0.06,
            elevation: 0.03,
            elevation_shot: 0.09,
            spin_rel: 0.2,
            first_time_factor: 1.3,
            error_scale_worst: 1.32,
            error_scale_best: 0.6,
            weak_foot_max_factor: 1.5,
            foot_outside_soft_deg: 60.0,
            foot_outside_hard_deg: 0.0,
            strong_foot_slack_s: 0.21,
            pressure_factor: 0.6,
            shank_probability: 0.03,
            shank_factor: 4.0,
            short_pass_full_m: 20.0,
            long_pass_full_m: 35.0,
            finishing_full_m: 12.0,
            long_shots_full_m: 22.0,
            setup_angle_soft_deg: 100.0,
            setup_angle_hard_deg: 40.0,
            setup_back_speed: 1.5,
            setup_timeout_s: 0.5,
            body_angle_factor: 1.0,
            setup_shot_decay_s: 1.0,
            first_time_open_deg: 60.0,
            shot_speed_ref: 0.8,
            shot_speed_exp: 2.0,
        }
    }
}

/// Logistic duel models.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DuelConfig {
    /// Slope of the logistic in effective-skill difference (skills normalised 0..1). Calibrated against the `layers`
    /// bench so that a squad 10 points better in everything is a clearly better side, not an unbeatable one.
    pub tackle_slope: f32,
    /// Reference skill (0..1) the difficulty terms of the one-sided duels (control, save) were calibrated with: at
    /// this skill the outcome is independent of the skill slopes.
    pub skill_pivot: f32,
    pub tackle_from_behind_penalty: f32,
    /// A ball between dribble touches that is nearer the defender than the carrier by this margin (m) is contested
    /// whatever the duel odds unless he comes from behind (IND-03's gate applies to a ball under control).
    pub tackle_loose_margin: f32,
    /// A defender acts on a ball that has come nearer to him than to the carrier (better odds, the loose-ball commit)
    /// only once it has been so for this many of his reaction times: he has to see it first.
    pub tackle_cue_reaction: f32,
    pub tackle_clean_share: f32,
    pub beaten_recovery_s: f32,
    pub tackle_duration_s: f32,
    /// Slope of the control logistic in the ball's difficulty …
    pub first_touch_slope: f32,
    /// … and in the player's control skill around `skill_pivot`.
    pub first_touch_skill_slope: f32,
    /// Skill margin at which an average reception is controlled (higher = fewer heavy touches).
    pub first_touch_offset: f32,
    /// First touch: ball speed relative to the player (m/s) at which the speed part of the difficulty reaches
    /// `first_touch_speed_weight`; it grows with the square of the speed (a firm pass is easy, a driven one is not).
    pub first_touch_speed_ref: f32,
    pub first_touch_speed_weight: f32,
    pub first_touch_pressure_weight: f32,
    /// Heavy touch: the ball comes off at this fraction of its speed relative to the player when the touch only just
    /// failed …
    pub heavy_touch_rebound_min: f32,
    /// A failed touch on an opponents' ball faster than `deflect_min_speed` (m/s, relative) is a deflection keeping
    /// `deflect_keep` (range) of its speed, instead of a heavy touch.
    pub deflect_min_speed: f32,
    pub deflect_keep: [f32; 2],
    /// … rising by up to this much for the worst touches (cubic in how badly it failed).
    pub heavy_touch_rebound_range: f32,
    /// Slope of the save logistic in the shot's difficulty …
    pub save_slope: f32,
    /// … and in the keeper's save skill around `skill_pivot`.
    pub save_skill_slope: f32,
    /// Log-odds of a save when the keeper's save skill equals the shot's difficulty.
    pub save_bias: f32,
    /// Aerial duel: slope of the logistic in aerial-skill difference …
    pub aerial_slope: f32,
    /// … plus this many log-odds per metre the winner reaches higher with a jump.
    pub aerial_reach_per_m: f32,
    /// An opponent within this horizontal distance (m) of a high ball that a player is about to play challenges him
    /// for it (IND-07; calibrated: an aerial duel ends 11–14 % of lofted 30–45 m passes, StatsBomb 14–15 %).
    pub aerial_contest_radius: f32,
    /// Share of headers won in an aerial duel that are headed away loose rather than to a team-mate, as this ×
    /// (1 − heading): a 68-rated header is loose 29 % of the time (StatsBomb open play, 40 matches: the winner's
    /// first action is a headed clearance in 29 %, a pass in 53 %, a shot in 10 % of lost duels).
    pub contested_header_loose: f32,
    /// Challenges (tackles, take-ons, aerial duels, shielding): strength counts this many rating points per kg of
    /// body weight above `weight_ref_kg` (`duel::challenge_strength`; calibrated).
    pub weight_strength_per_kg: f32,
    pub weight_ref_kg: f32,
    /// Shots struck nearer than this (m from goal) start to become a direct duel with the keeper: the weight of his
    /// save skill moves from reflexes to one-on-ones …
    pub save_one_on_one_far: f32,
    /// … completely by this distance (calibrated).
    pub save_one_on_one_near: f32,
    /// A parry goes behind the goal line, round the post or over the bar, with this probability (else out in front;
    /// calibrated: StatsBomb, a save precedes 10 % of corners, ≈ 19 % of saves give a corner).
    pub parry_behind: f32,
}

impl Default for DuelConfig {
    fn default() -> Self {
        Self {
            tackle_slope: 2.5,
            skill_pivot: 0.58,
            tackle_from_behind_penalty: 1.0,
            tackle_loose_margin: 0.3,
            tackle_cue_reaction: 1.0,
            tackle_clean_share: 0.6,
            beaten_recovery_s: 0.8,
            tackle_duration_s: 0.35,
            first_touch_slope: 5.0,
            first_touch_skill_slope: 2.5,
            first_touch_offset: 0.6,
            first_touch_speed_ref: 20.0,
            first_touch_speed_weight: 0.6,
            first_touch_pressure_weight: 0.25,
            heavy_touch_rebound_min: 0.06,
            deflect_min_speed: 10.0,
            deflect_keep: [0.3, 0.7],
            heavy_touch_rebound_range: 0.25,
            save_slope: 4.0,
            save_skill_slope: 2.0,
            save_bias: -0.15,
            aerial_slope: 2.5,
            aerial_reach_per_m: 3.0,
            aerial_contest_radius: 1.6,
            contested_header_loose: 0.9,
            weight_strength_per_kg: 1.0,
            weight_ref_kg: 75.0,
            save_one_on_one_far: 16.5,
            save_one_on_one_near: 8.0,
            parry_behind: 0.6,
        }
    }
}

/// Laws of the Game (M7) [IFAB]: added time, fouls and misconduct, advantage, substitutions. Rates are calibrated
/// against StatsBomb open data (40 matches, per team-match: 12.1 fouls, 2.1 cautions, 0.04 sendings-off, 1.6
/// advantages, 0.11 penalties, 4.4 substitutes), scaled by the engine's longer ball-in-play time.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// Law 7.3: time the referee allows in each half for losses the engine does not play out (injuries, time-wasting,
    /// "any other cause"; s) …
    pub added_base_s: f32,
    /// … for each goal (the celebration), each substitution, each caution or sending-off and each penalty kick (s).
    pub added_goal_s: f32,
    pub added_sub_s: f32,
    pub added_card_s: f32,
    pub added_penalty_s: f32,
    /// Law 12.1: the probability that a challenge is a foul by its outcome: the tackler beaten (he missed the ball), the
    /// ball knocked loose, the ball won …
    pub foul_beaten: f32,
    pub foul_deflected: f32,
    pub foul_won: f32,
    /// … × this from behind …
    pub foul_from_behind: f32,
    /// … × (1 ± this) from the best to the worst tackler and from the calmest to the most aggressive player.
    pub foul_tackling: f32,
    pub foul_aggression: f32,
    /// The loser of an aerial duel fouls the winner (holding, pushing) with this probability.
    pub foul_aerial: f32,
    /// Law 12.3: a foul is reckless (a caution) with this probability, × `card_from_behind` from behind and × (1 ±
    /// `foul_aggression`) by aggression; it uses excessive force (a sending-off) with `red_excessive`.
    pub card_reckless: f32,
    pub card_from_behind: f32,
    pub red_excessive: f32,
    /// A foul on a carrier heading for goal within this distance of it (m) stops a promising attack (SPA, a caution)
    /// when at most `spa_defenders` outfield defenders besides the offender are goal-side of him …
    pub spa_distance: f32,
    pub spa_defenders: usize,
    /// … and denies an obvious goal-scoring opportunity (DOGSO) within `dogso_distance` with none goal-side: a sending-
    /// off, a caution in the penalty area (every engine foul is a challenge for the ball).
    pub dogso_distance: f32,
    /// Law 5.3 advantage: a fouled carrier who beat the challenge stays on his feet with probability `stay_min` +
    /// `stay_range` × strength; in his attacking half the referee then lets play go on and penalises the foul if his
    /// team loses the ball within `advantage_s` ("at that time or within a few seconds").
    pub stay_min: f32,
    pub stay_range: f32,
    pub advantage_s: f32,
    /// A fouled player who goes down is on the ground this long (s).
    pub fouled_down_s: f32,
    /// Layer 3: a cautioned player commits to a tackle only at this much better win probability, and never dives in;
    /// his challenges are fouls `caution_care` × as often, reckless `caution_care` × as often, and he commits no
    /// tactical fouls.
    pub caution_margin: f32,
    pub caution_care: f32,
    /// IND-03 in the own penalty area: a defender's challenges there are fouls `foul_box` × as often (he stays on his
    /// feet; StatsBomb: 0.9 % of fouls are penalties).
    pub foul_box: f32,
    /// Law 3: substitutes per team and substitution opportunities besides half-time in official competitions.
    pub max_subs: u32,
    pub max_sub_windows: u32,
    /// Default substitutions (until the manager makes them): from `sub_from_min`, at a stoppage, once an outfield
    /// player's energy reserve is below `sub_reserve` + `sub_reserve_per_min` × (minutes past `sub_from_min`), the
    /// team replaces those below that + `sub_bundle`, the most tired first, at most `sub_per_window` at once; a
    /// cautioned player counts `sub_caution` lower. Calibrated to StatsBomb's substitution minutes (p10 / median / p90
    /// 51 / 72 / 87, 4.4 per team).
    pub sub_from_min: f32,
    pub sub_reserve: f32,
    pub sub_reserve_per_min: f32,
    pub sub_bundle: f32,
    pub sub_per_window: u32,
    pub sub_caution: f32,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            added_base_s: 60.0,
            added_goal_s: 60.0,
            added_sub_s: 30.0,
            added_card_s: 20.0,
            added_penalty_s: 60.0,
            foul_beaten: 0.4,
            foul_deflected: 0.155,
            foul_won: 0.04,
            foul_from_behind: 2.0,
            foul_tackling: 0.5,
            foul_aggression: 0.5,
            foul_aerial: 0.05,
            card_reckless: 0.15,
            card_from_behind: 2.0,
            red_excessive: 0.002,
            spa_distance: 35.0,
            spa_defenders: 1,
            dogso_distance: 25.0,
            stay_min: 0.2,
            stay_range: 0.4,
            advantage_s: 3.0,
            fouled_down_s: 1.5,
            caution_margin: 0.1,
            caution_care: 0.15,
            foul_box: 0.13,
            max_subs: 5,
            max_sub_windows: 3,
            sub_from_min: 45.0,
            sub_reserve: 0.58,
            sub_reserve_per_min: 0.0022,
            sub_bundle: 0.04,
            sub_per_window: 2,
            sub_caution: 0.1,
        }
    }
}

# Match engine — design notes

A headless, deterministic 11 v 11 match simulator, the foundation of a tactics-focused football manager.
Rendering is kept out of the engine so the 2D viewer can later be replaced by a 3D front end.

## Crates

| Crate | Kind | Purpose |
|---|---|---|
| `crates/engine` (`match_engine`) | library | Simulation, AI, physics, stats. No rendering dependencies. |
| `crates/viewer` (`match_viewer`) | binary (eframe/egui) | Watch a match with tactical overlays; ball-physics sandbox. |
| `crates/lab` (`match_lab`) | binary (rayon, clap) | Plays N seeded matches in parallel; prints statistics, the calibration metrics and a scorecard; runs A/B experiments; learns the possession value by self-play. |

```
cargo run --release -p match_viewer [seed]          # watch a match (space = play/pause, click a player)
cargo run --release -p match_viewer -- --sandbox    # ball physics sandbox
cargo run --release -p match_lab -- --matches 100   # batch statistics + calibration report + scorecard
cargo run --release -p match_lab -- --experiment data/experiments/high_line_fast_strikers.ron --matches 500  # A/B experiment
cargo run --release -p match_lab -- --matches 300 --learn-value value.ron  # possession value by self-play
cargo test --workspace                              # unit, principle-scenario and whole-match tests
cargo run --release -p match_engine --example film -- [seed] [out.jsonl] [interval_s]  # record a match as JSON lines (frames + events) for offline viewing
cargo run --release -p match_engine --example <name> # diagnostics: shots, saves, goals, defending, finishing, box, spell, duels, keeper, tackles, turnovers, dribbles, block, decisions, snapshot, passes, long_choice, long_defend, pressed_cal, pass_cal, lane_cal, high_balls, support, layers, intercepts, free_risk, styles, receptions, carry_cal, space_losses, hold_cal, shot_share, final_third, forward_moves
```

The lab accepts `--config file.ron` with `SimConfig` overrides, `--wet`, `--half-minutes`, `--events`, `--home` /
`--away` (a builtin squad or a `TeamData` RON file), `--experiment file.ron` and `--learn-value out.ron` (M8).
The viewer also accepts `--at <match seconds> --select <player id> --shot <file.ppm>` for screenshots.

## Conventions

- SI units. Pitch 105 × 68 m, origin at the centre spot, x along the length, y across (+y to the left of a team
  attacking +x), z up. Players are `Vec2`, the ball `Vec3` (glam).
- **Team frame:** all tactical code works in a frame where the team attacks +x (`TeamFrame`, a sign flip of x and y).
  Teams change ends at half-time by flipping their frame.
- Fixed tick of 50 Hz (`timing.dt` = 0.02 s), ball sub-stepped 4×. Player brains update every 0.2 s and at pass
  release; individual decisions are staggered per player (decision interval from attributes, layer 3).
- **Determinism:** one seeded ChaCha8 RNG per match, fixed player order, no hash-map iteration in the state.
  The same seed gives the same event log (`events::hash_events`; checked by `tests/match_sanity.rs`).
- All tuning constants live in `SimConfig` (`config.rs`). Tactical parameters are named after the catalog rule
  that introduces them (`def02_cover_distance`, `prs07_front_step`, …).

## Tick order (`Simulation::step`)

1. Phase handling (kick-off, restarts, goal celebrations).
2. Player brains every 0.2 s (`ai::brain::update` for each player, over the world cache `ai::world::World`) and
   calibration sampling (`stats`).
3. Off-ball decisions (`ai::positioning::decide`) for players whose decision time has come.
4. Ball carrier (`carrier`): carry / hold / on-ball decision (`ai::on_ball`), kick execution (`ai::execution`).
5. Goalkeepers (shot reaction, dives).
6. Locomotion for all players, then soft separation.
7. Ball attached to a holding player, else ball physics (`ball::physics::step_ball`).
8. Contacts (receptions, interceptions, headers, tackles, keeper saves) and the referee (goals, out of play).

## Three layers of behaviour

| Layer | Off the ball | On the ball |
|---|---|---|
| 1 — tactical optimum | Player brains (phase, block, duties) + scored candidate points around the formation anchor. **Every term is a catalog principle.** | `U_opt = p·V(success) − (1−p)·C(turnover)` with the catalog's tactical bonuses (ATT-01/02/03/04/06/17/21, TR-01). |
| 2 — manager instructions | `TeamInstructions` / `PlayerInstructions`, every field `Option` (`None` = trust layer 1). Resolved in `team::tactics`. | Instruction bias (directness, tempo, dribble frequency), applied in proportion to discipline and role familiarity. |
| 3 — the individual | Positional drift (Ornstein–Uhlenbeck), reaction lag and score noise scaled by 1 − tactical reliability, where reliability = intelligence × confidence × energy × role-familiarity factors (`ai::deviation::reliability`; exactly 1, so no deviation, only for a 100 at full confidence, energy and familiarity). Players commit to a chosen point (switching costs `commit_cost` per metre) and ignore corrections within `settle_radius`. | Personality bias (egotism, flair, bravery / aggression, damped by teamwork; `ai::personality`), softmax noise with a temperature from decisions × the same confidence and energy factors, zero for a 100 at full confidence and energy, raised by pressure for the less composed (held while on the ball, see below), then execution error (`ai::execution`): the physics decides the outcome. |

All on-ball biases and the softmax temperature are fractions of the **local value scale** (possession value at the
ball), so a personality or instruction weighs the same in the own third as in the box.

**Defending parameters are per team.** `ResolvedTactics::defending` (`DefendingTactics`) holds every value that sets a
style of defending:
- block line and length (BLK-05, DEF-05), width (DEF-05) and ball-side shift (DEF-06);
- the BLK-02 gaps behind the ball;
- box defending: BOX-04 range, back-line gap, shift, midfield distance and drop urgency; BOX-01 contain distance;
  BOX-02 block distance; BOX-03 cover radius and tackle threshold.

Layer 1 takes each value from its catalog parameter in `TacticsConfig`. A line or length follows the block height.
`TeamInstructions::defending` (`DefendingInstructions`) overrides any of them in engine units, clamped to a plausible
range. These are the raw layer-2 values; the manager-facing options of M5 (e.g. "narrow", "engage early in the box")
will set them.

Each player's brain builds the layer-2 block from the team's values and the layer-1 block from the catalog values,
and his anchor blends the two by adherence. Rules that act per player (BOX-01 contain distance, BOX-02 block
point, BOX-04 drop urgency, BOX-03 cover and tackle threshold) use `applied_defending`: the catalog values moved
towards the team's values by the same adherence (discipline, role familiarity).

The in-possession values are per team in the same way: `ResolvedTactics::attacking` (`AttackingTactics`) holds the
ATT-24 distance a pressed receiver comes towards the passer and the ATT-25 turnover share expected of a pressed
player on the ball. `TeamInstructions::attacking` (`AttackingInstructions`) overrides them, and `applied_attacking`
blends them by adherence. They will back M5 options such as "come short for the ball" and "release the ball early".

Definitions and physical limits stay global in `TacticsConfig`:
- the shooting range (`box01_range`);
- the pressure radius;
- line speed (`def05_shift_speed`);
- the TRN-03 squeeze;
- press zones.

The kickoff shape uses each team's own width and box values at the mid-block line and length.

**Decision noise persists.** The carrier re-decides every `onball_interval` (0.3 s). If every re-decision drew a
fresh sample from the softmax, any poor option would win sooner or later. A 30 m shot taken 5 % of the time per
decision becomes likely over a three-second carry, and dribbles flip direction from one touch to the next. So
`on_ball::decide` samples with the Gumbel-max trick, and each option's Gumbel draw is fixed by the player's
`decision_seed` (new at every gain of control) and the option's label. A misjudgement then lasts for the spell on the
ball, and is drawn afresh after `decision_noise_hold_s` (2 s, calibrated) or when the pressure on him moves to
another of `decision_noise_pressure_bands` (3): a defender closing in is a new situation, judged anew (see "Carry in
space"). A single decision still follows the softmax probabilities exactly. The placed, power and curled shot variants share one alternative's weight, so listing more
ways to shoot does not make shooting more likely. The dribble directions share one weight in the same way (see
"Pressed receptions").

**xG** (`analysis::shot_xg_parts`) is a chain of factors:
- **Base curve:** `logistic(xg_a + xg_b·goal angle − xg_c·distance − xg_lateral·|y|)`. It assumes a keeper set on
  his GK-01 spot, nobody in the way and a reference finisher striking a controlled ball with his strong foot. It is
  a logistic regression (a one-off analysis, not in the repository) on the clean open-play footed shots in [StatsBomb
  open data](https://github.com/statsbomb/open-data): 610 matches of the Premier League 2015/16, the World Cups 2018
  and 2022 and the Euros 2020 and 2024, 12,129 footed open-play shots. "Clean" means no opponent in the
  shooter–posts triangle and the keeper within 1.5 m of the shot line and < 6 m off his line in the freeze frame
  (4,605 shots, 1,572 of them first time). The fit has weak-foot and first-time terms, so the curve is the
  controlled strong-foot shot: ≈ 0.55 at 6 m, 0.30 at 11 m, 0.159 at 16.5 m, 0.105 at 20 m, 0.058 at 25 m and 0.032
  at 30 m, all central; 0.046 from 16.5 m out and 12 m wide, 0.013 from 25 m out and 15 m wide. A first-time shot
  is −0.21 ± 0.12 in logit, 0.82–0.90 of a controlled one. The finishing factor below prices it at 0.84. Without the
  first-time term the curve mixed in the 46–83 % first-time shots of the 0–16.5 m bands (real) and underrated the
  engine's mostly controlled shots by 5–10 % in the box. The lateral term (−0.065 per metre off the centre line,
  ± 0.016) is significant on top of the goal angle: central shots convert better and wide shots worse than their
  angle alone says. The real curve by distance band (all footed open-play shots, any situation)
  is 44 % (0–6 m), 23 %, 12.7 %, 4.8 %, 2.6 % and 0.8 % (30–40 m).
- **Blockers:** a factor for each opponent in the shooting triangle.
- **Keeper:** an open-goal term. For nine aim points across the goal mouth, the actual keeper races the ball (at
  `xg_shot_speed_factor` of the shooter's kick speed) to its path, anywhere up to the line. He needs his reaction
  time, must finish whatever he is doing, then runs and dives; the race is soft (`xg_keeper_race_sigma`). The
  shooter's set-up delay (`ShotHow::lead`) counts against the keeper's time. xG rises towards the open-goal value by
  the share of the goal he leaves open beyond what a keeper set on his GK-01 spot would leave. The open-goal value
  falls steeply with the goal angle (`xg_open_goal`·min(angle / `xg_open_angle`, 1)^`xg_open_exp`: 0.92 from close
  in, ≈ 0.24 from 30 m central), so a keeper caught off his line does not turn every 30 m shot into a 0.2 chance. In
  StatsBomb, 22–40 m shots convert 1.6 % with the keeper < 2 m off his line, 2.7 % at 2–4 m, 3.4 % at 4–6 m and
  ≈ 12 % at 6–15 m (n = 34).
- **Pressure:** the closest opponent within 6 m (`pressure_at`; one behind the ball counts 0.7) cuts xG by up to
  `xg_pressure_factor` (0.35).
- **Finishing:** `(reference error scale / shooter's error scale)^xg_finish_exp`. Both are `ExecInput` error scales
  for this shot. The shooter's scale uses his finishing / long-shot / technique skill, the foot he strikes with
  (`weak_foot_max_factor` for the weak foot), first-time, and the body angle left over at the kick. The reference is
  a finisher of skill `xg_ref_skill` (0.65, so that the mean factor at shots is ≈ 1) on his strong foot, set,
  confident and fresh. The exponent (0.65) and `weak_foot_max_factor` (1.5) were set with the `finishing` bench
  (placed shots from fixed spots against a set keeper): skill 16 converts ≈ 1.5 times what skill 6 does, and the
  weak foot ≈ 0.84 of the strong foot, where xG says 0.85. In StatsBomb, weak-foot shots by one-footed players
  (≥ 70 % of their passes with one foot, ≥ 60 passes; 21.7 % of their shots) score 0.85 times the goals per
  StatsBomb xG of strong-foot shots (logit −0.21 ± 0.09). The gap grows with distance: 0.92 below 11 m, 0.6 beyond
  22 m on few goals. The side of the pitch does not interact with the foot. That fits [Marcori et
  al.](https://lida.sport-iat.de/ta/Record/4078349) (five European leagues: foot choice is asymmetric, performance
  differences are small), the [StatsBomb footedness
  analysis](https://blogarchive.statsbomb.com/articles/soccer/footedness-and-why-erik-lamela-is-worth-30m/) and
  [Frontiers 2024](https://www.frontiersin.org/journals/sports-and-active-living/articles/10.3389/fspor.2024.1434096/pdf)
  (preferred vs non-preferred foot shot dispersion 6.17 vs 10.22 m²). [Bayes-xG](https://arxiv.org/pdf/2311.13707)
  also models the foot.

A long shot against a set keeper is therefore worth ≈ 0.01–0.03. One against a keeper caught off his line is worth
more. The earlier rule used the keeper's offset from the shooter–goal-centre line, and that gave 30–40 m shots
≈ 0.12.

The shot options (`on_ball::shots`) consider the strong foot for each variant and, if it strikes sooner or with less
body angle left over, the weak foot too, priced with the xG of that foot.

## Tactical knowledge base

`docs/tactics/principles.md` is the source of truth for layer 1: each entry has an ID, the source's words, our
translation, and parameters tagged `sourced` or `calibrated`; `docs/tactics/sources.md` is the bibliography.
`tests/catalog.rs` fails if a `v1` principle is not referenced in the engine source. The viewer shows, for the selected
player, the principle IDs behind his current target or decision with their catalog titles (hover for the source text).

## Ball physics

Quadratic drag, Magnus force from spin (+z spin curls left), spin decay, bounces with restitution and a tangential
friction impulse coupled to spin (topspin skips, backspin checks), rolling resistance from grass length and wetness,
posts and crossbar as cylinders, a seeded bumpiness field that tilts bounces and nudges rolling balls.
`ball::trajectory::predict` uses the same step function without the surface noise; the AI plans with it, and
`solve_kick` inverts it for passes, lofted balls, through balls and shots.

## Body, movement and touch

- **Off-ball movement.** Positional urgency grows with distance from the point (`position_full_speed_dist`, floor
  `position_min_urgency`): players walk or jog when they are roughly in place and run only when far off. A chosen
  block or support point is kept unless another is clearly better (`commit_cost` per metre of switch), small
  corrections within `settle_radius` are ignored, the block shifts at most `def05_shift_speed`, and DEF-04 marks are
  only released once the opponent leaves the zone by `def04_release_margin`. The body faces the ball when walking or
  jogging and turns into the run direction between 3 and 5.5 m/s.
- **Kick set-up.** A kick can be struck when the body is within `execution::setup_angle` of the kick (≈ 100° for a
  soft pass with the inside of the foot down to ≈ 40° at full power) and the kicker is not moving away from it by
  more than `setup_back_speed`. Otherwise he first plants and turns with the ball at his feet (`Intent::SetUp`, body
  turn rate `ball_turn_rate` by quickness and dribbling), where he can be tackled. The on-ball AI prices that delay
  in (`setup_time`): opponents get the extra time in the interception race, and shots lose xG by
  `exp(−t / setup_shot_decay_s)`. Any angle left over when the kick is struck adds execution error
  (`body_angle_factor`). Restart takers stop at the ball and line up first.
- **Feet.** Each foot kicks across the body: the right foot strikes to the left of the body line (inside of the
  foot, or the instep from an angled run-up, whose angle is one of the determinants of instep kick speed: [Kellis &
  Katis 2007](https://pmc.ncbi.nlm.nih.gov/articles/PMC3786235), *J Sports Sci Med* 6:154–165), and only a little to
  its own side: `foot_outside_soft_deg` (60°) for a soft kick down to `foot_outside_hard_deg` (0°) at full power. This
  is a calibrated simplification. A kick beyond that is struck by the other foot (`execution::natural_foot`), or the
  body turns further first (`foot_cone`, `setup_turn`). Before, the rule was the other way round (the right foot
  took kicks to the right). Passes and clearances set up on the strong foot unless that takes more than
  `strong_foot_slack_s`·(1 − weak-foot ability) longer than striking with whichever foot can (`Ctx::pass_delay`).
  First-time kicks use whichever foot the ball arrives at. The weak foot multiplies the execution error by up to
  `weak_foot_max_factor` (1.5, at weak-foot ability 0). Calibration: one-footed players in StatsBomb play 13.1 % of
  their passes with the weak foot; the engine plays 14 % (`strong_foot_slack_s` 0.21; at 0.15 it was 21 %, at 0.3
  2 %).
- **First touch.** Control difficulty grows with the square of the ball's speed relative to the player
  (`first_touch_speed_ref`, `_weight`), with height and pressure. A failed touch knocks the ball back off the shin or
  past the player at a fraction of its relative speed that grows with how badly the touch failed
  (`heavy_touch_rebound_min` + `_range`·badness³, less for a good first touch), so most heavy touches stay within
  a stride or two. The contact uses the ball's 3D velocity relative to the player and an effective height that adds
  0.6 m per 5 m/s of vertical speed, the same terms the pass evaluation (`evaluate_pass`) prices in.
- **Settling the ball.** After a controlling touch, and after a knock-on, the carrier closes the gap to the ball
  within `settle_catch_s` (0.4 s) instead of at dribbling pace, and he takes no knock-on touch when his next decision
  is due before he could touch the ball again. Running onto it, he can play it (decide, pass) with his next step:
  within `control_radius` + `run_reach_s` (0.1 s) × his speed towards it. The next dribble touch waits until it is at
  his feet. The controlling touch goes only where he can turn his run to within `touch_follow_s` (0.5 s)
  of lateral acceleration (`touch_cone`); a wanted direction outside it is taken to its edge. The intended receiver
  brakes when he would reach the reception point early, and an uncontested receiver lets a dropping pass he planned
  to take lower fall to him instead of heading it as it comes down through head height.
- **Sprint.** A player's acceleration falls linearly from `accel` at rest to 0 at top speed, so a sprint follows
  the mono-exponential model v(t) = v_max·(1 − e^(−t·a/v_max)) used in sprint profiling, with `accel` ≈ F0 (N/kg).
  Amateur players: F0 8.08 N/kg, V0 8.98 m/s, 5 m in 1.32 s, 10 m in 2.06 s, 20 m in 3.33 s, 30 m in 4.50 s from
  first movement ([Mitrečić & Vučetić 2025](https://mjssm.me/clanci/MJSSM_March_2025_Mitrecic.pdf), *MJSSM*,
  doi:10.26773/mjssm.250307); professionals run 30 m in ≈ 4.15 s
  ([PMC9819841](https://pmc.ncbi.nlm.nih.gov/articles/PMC9819841/)). Attributes map linearly to `top_speed` 7.4–9.8
  m/s and `accel` 6.6–10.0 m/s² (7.0–10.0 and 6.0–10.5 until the attribute-slope round of 2026-10-29 narrowed them
  around the same average player). The squads' average player (pace and acceleration ≈ 13.4) now runs 10 m in ≈ 2.0 s
  and 30 m in ≈ 4.3 s after his reaction time. Before (6.8–9.6 m/s, 3.5–6.5 m/s²) he needed ≈ 2.4 s and ≈ 5.0 s, so
  every race for a ball, a press and a recovery run was ≈ 40 % too slow over the first metres.
- **High balls.** An outfield player can play a ball up to his header height (`jump_reach` − 0.25 m). The team
  brain's chaser and the pass evaluation use the same reach, so a lofted pass can be contested in the air. When a
  ball above `control_height` reaches a player and an opponent who can also reach it stands within
  `aerial_contest_radius` (1.6 m), it is an aerial duel. The winner heads it; the loser cannot touch the ball for
  0.4 s. Win probability = logistic(`aerial_slope`·Δskill + `aerial_reach_per_m`·Δjump reach), with skill = 0.4
  heading + 0.25 jumping + 0.2 challenge strength + 0.15 aggression (calibrated weights; `duel::aerial_win_prob`;
  challenge strength is strength plus `weight_strength_per_kg` per kg above `weight_ref_kg`).
  In StatsBomb an aerial duel ends 14–15 % of high passes of 30–45 m (166 matches); the engine 11–14 % since
  IND-07 (it was ≈ 1–4 %; see "Contested high balls and lofted accuracy" below).
- **Pass weight.** A ground pass to feet is solved to arrive at `pass_arrival_base` + `pass_arrival_per_m`·length
  m/s, within `pass_arrival_min`..`_max` (4 + 0.38/m, 6–12 m/s). Calibrated on StatsBomb's `duration` (release to
  reception) of completed open-play ground passes, banded by the distance to where the ball was received:

  | band | StatsBomb median | engine before (5–9 m/s, 4 + 0.12/m) | 4 + 0.3/m, 6–9 m/s | engine now (with ATT-24) |
  |---|---|---|---|---|
  | 0–10 m | 0.90 s | 0.96 s | 0.82 s | 0.86 s |
  | 10–20 m | 1.25 s | 1.58 s | 1.28 s | 1.30 s |
  | 14–18 m | 1.34 s | 1.62 s | 1.36 s | 1.30 s |
  | 20–30 m | 1.70 s | 1.96 s | 1.84 s | 1.64 s |
  | 30–45 m | 2.24 s | 2.16 s | 2.18 s | 2.30 s |

  The pass is weighted for the receiver to meet it without breaking stride (ATT-24, [FIFA-TCP]). Once receivers came
  to meet pressed passes, the long bands came out short at 6–9 m/s, so the slope and the cap were raised.

  Released at ≈ 13 m/s, a 16 m pass now arrives at ≈ 10 m/s, which a receiver controls with p ≈ 0.99 unpressed.
  Before, it left the foot at ≈ 9 m/s and took ≈ 2.2 s to a standing receiver, so with realistic sprinting the keeper
  and a covering defender could race it (`tests/personality.rs` failed on exactly that). Firmer passes also made
  short and medium ground passes more attractive to the on-ball AI: 10–20 m ground passes rose from ≈ 123 to ≈ 210
  per match. A tracking study of one club estimated a typical initial pass speed of 16.2 m/s ([Controlling ball
  progression in soccer](https://arxiv.org/pdf/2210.16474)).
- **Lofted passes.** `evaluate_pass` prices a lofted pass to a team-mate as the sim settles it: a first-arrival race
  along the ball's path, from the kick to `lofted_race_tail_s` (1.2 s) after the landing. The receiver can play it
  wherever he reaches it below control height, an opponent below his header height, and a ball that leaves the pitch
  first is lost. The race is averaged over five landing points spread by the kicker's own execution error
  (`execution::landing_spread`: the aimed point and ±√3 σ along and across the kick). The along spread comes from
  the solver's landing distance per m/s (`KickSolution::range_per_speed`, drag and spin included): drag cuts the
  drag-free sensitivity of 2 to 1.25–1.7. Over 400 perturbed kicks it lies within 20 % of the flown spread (test
  `landing_spread_matches_flown_kicks`). In matches, lofted passes land rms 3.2 m along and 1.7 m across from the
  target (`lofted` example). Since 2026-10-29 ground passes to a team-mate use the same race, without the spread
  (see "Reading the pass" below).

  The `lofted` example also exposed a receiving fault. A pressed receiver met the ball as for a ground pass (ATT-24),
  running through the drop point without braking. The ball bounced on behind him at 8–12 m/s, out of play or to an
  opponent. A dropping ball is now met where it drops. Predicted p against the share the receiver controls
  (`pass_speed`, 20 matches):

  | predicted p | lofted before | lofted now | ground now |
  |---|---|---|---|
  | 0.80–0.90 | 52 % | 78 % | 83 % |
  | 0.90–0.95 | 66 % | 88 % | 88 % |
  | ≥ 0.95 | 84 % | 96 % | 94 % |

  Lofted passes out of play: 9 % → 2 %. Receiver controls lofted 30–45 m passes: 66 % → 78 % (StatsBomb high passes
  of that length: 54–73 % completed).
- **Pursuit.** A movement target can itself move (`Player::target_vel`): it advances at that velocity between
  decisions, and arrival braking limits only the closing speed relative to it; the sum is capped at the player's top
  speed. The first defender's contain point (IND-01), the DEF-02 cover point and the BOX-02 block point move with the
  carrier. Before, a defender braked for a contain point 2–4 m away as if it stood still and fell behind a carrier
  running at 7–8 m/s.
- **Tracking.** A defender whose target follows the carrier (`Move::tracks`: the first defender within
  `ind01_track_distance`, the cover) re-aims at his reaction time instead of his decision interval, so he mirrors the
  carrier's changes of direction with a human delay rather than up to a second late.
- **Meeting a moving point.** `ai::reach::time_to_meet` is the earliest time (sampled every 0.1 s up to 2 s) at
  which a player can be where a moving point will be. Beyond the horizon it is the time to the point's position
  then. A chaser from behind never meets the contain point of a carrier running faster than he can close, so a
  defender in the path ranks first (DEF-03).
- **Goalkeeper dive.** After his reaction time (`gk_reaction_max` → `_min` by reflexes) the keeper plans the dive
  from a fresh prediction of the ball, so a sample's time is the time he has left. He picks the point on the path
  (inside his box, below `gk_max_height`) with the most margin: the dive he can manage by then
  (`gk_dive_speed`·t, at most his dive reach) minus how far he must move to have the ball 0.3 m inside his standing
  reach. He dives only that far, only as fast as the time left demands. The save roll (`duel::save_prob`) takes
  that speed as a fraction of `gk_dive_speed` as its reach term, so a ball he had time for is easy and a full-stretch
  dive is hard. Before, he dived to the first point he could reach at all. That point is at the edge of his reach by
  construction, so every dive counted as full stretch. He also planned with a trajectory up to one team tick old,
  so he thought he had up to 0.2 s more than he did.
- **Shot speed–accuracy.** Shot dispersion grows with how hard the ball is struck: the execution error scale is
  multiplied by (speed fraction / `shot_speed_ref`)^`shot_speed_exp`, both for aim and elevation. The form follows
  Hunter et al. 2018 ("Modeling the two-dimensional accuracy of soccer kicks", *J Biomech* 72:159–166,
  doi:10.1016/j.jbiomech.2018.03.003: 8,466 kicks, horizontal and vertical variance a power function of ball speed,
  low targets more precise than high). The exponent is calibrated, because the abstract gives no values. A placed shot
  (0.8 of the kicker's maximum) keeps the base error; a full-power one has ≈ 1.5 times as much. Shots have a larger
  elevation error (`elevation_shot`) than passes, so some go over the bar.
- **Carrying speed.** The carrier runs to his knocked-on ball at urgency `carry speed / top speed + 0.1`, so he
  dribbles at about 0.75–0.85 of his top speed. With `+ 0.25` he could sprint to every touch and carried the ball at
  up to his full top speed (8–9 m/s), faster than defenders turning to follow him. Dribble control is unchanged
  (`dribbles` example: the same share of touches kept per turn angle).
- **Shot blocks.** A shot that passes within `body_radius` (0.45 m) of an outfield opponent hits his body with
  `shot_body_block_p` (0.9) and rebounds like a heavy touch. Further out, a stretched leg or a slide reaches a low
  ball (bottom ≤ `shot_block_height`, 0.7 m) within `shot_block_reach` (1.4 m) with `shot_leg_block_p` (0.5) ×
  (0.75 + 0.25·anticipation). Both are calibrated to the StatsBomb blocked share by opponents in the
  shooter–posts triangle (see "Keeper position, saves and blocks" below). An opponent meets a shot where it passes
  closest to him: before, he rolled for the touch on the first tick the ball came within reach. A missed roll left
  him on touch cooldown when the ball reached his body, so body blocks happened about half as often as they
  should.

## Viewer

- **Match:** play / pause / step / speed 0.25–16×, new match / replay. Overlays: role labels, movement targets and
  anchors, the defensive block, the carrier's options coloured by success probability with choice percentages,
  pitch control, ball trail and predicted path. Live sliders for pitch conditions and both teams' instructions
  (unchecked = layer-1 optimum). The player panel shows the role (name and catalog ID), the brain duty, intent, confidence / role familiarity, the catalog
  principles behind the current target, the last on-ball decision broken down into layers 1 / 2 / 3 and the final
  probabilities, attributes and player instructions.
- **Sandbox:** press to place the ball, drag to aim (length = power), release to kick; sliders for loft, sidespin and
  topspin; pitch presets. Shows the straight aim line, the prediction on an ideal pitch, the actual trail and bounces.
- **Height** is shown three ways: the ball is drawn raised above its shadow (oblique view), a stalk joins them, and the
  elevation strip under the pitch plots height against x on the same horizontal scale (crossbar marked).

## Tests

| File | What it checks |
|---|---|
| unit tests in `src/` | physics (bounce decay, wet roll, curl side, posts, flat pitch), kick solver accuracy, `time_to_reach` vs locomotion, execution error vs skill and weak foot (and a full-power shot ≥ 1.3 times as wide and high as a placed one), foot geometry (each foot kicks across the body; a soft pass may go further to the foot's own side than a full-power shot), duel monotonicity, xG / value sanity (StatsBomb bands: 0.4–0.6 at 6 m, 0.2–0.35 at 11 m, 0.06–0.12 at 20 m, 0.02–0.045 at 30 m central; < 0.06 from 16.5 m out and 12 m wide), personality bias, instruction overrides, pursuit of a moving target, time to meet a moving point (a player in its path meets it, a chaser cannot) |
| `tests/catalog.rs` | every catalog entry has a valid ID and status; every `v1` ID appears in the engine source |
| `tests/principles.rs` | DEF-02 cover behind and inside (and still goal-side of the ball once the presser is beaten), DEF-03 a defender in a running carrier's path takes over from a chaser, and an engaged first defender keeps the role until the ball is past him, DEF-06 ball-side shift, IND-04 marking distance, ATT-10 staggering, ATT-14 cover shadows, TR-05 rest defence (4, or 5 against five high opponents), TRN-02 counter-press window, GK-01 depth within the StatsBomb keepers' interquartile range for every distance band, GK-02 smother only when first to the ball, GK-03 / BU-01 the keeper stays in goal with his own team on the ball within shooting range (and supports high beyond it), BOX-01 tight and showing outside in shooting range, BOX-02 cover on the shooting line, BOX-03 a covered defender engages an equal carrier (an uncovered one contains), BOX-04 back line at most 6 m gaps and inside the box width with the midfield 6 m in front (ball 18 m out; full width in midfield), and the line dropping at the ball's speed near goal (at most the DEF-05 cap with the ball still), DEF-04 a runner leaving the zone towards goal is tracked (goal-side) until a team-mate goal-side of him takes him over, BLK-02 the line leader's call to step changes only once the pressure has held for `blk02_call_hold_s`; phase D (individual brains): ATT-18 with an open ball a fast 9 runs in behind, waits onside and the through ball goes into his run; ATT-26 a pressed carrier gets an offer to feet instead of a run, a runner values the run and a link player the ball to feet (same situation), `forward_runs` / `come_short` change the choice, when one striker comes short the other holds the depth; look-ahead: a reader re-chooses for the next carrier as the pass is struck; phase E: ATT-10 and ATT-14 with perfect readers (no screened offer, never in a cover shadow), test 4 strikers in the midfielders' shadow go deep and the far midfielder offers in the gap, test 5 the ten does not copy the striker's run and drops into the space it opens, test 7 the centre-back in the build-up has three options on both sides, test 14 the full-back's `overlap`, `invert` and `forward_runs` instructions change his movement (released by default, overlap / underlap, invert, claims rest defence) |
| `tests/dribbling.rs` | an unchallenged carrier running at 6 m/s keeps the ball through turns of 0–180° |
| `tests/kicking.rs` | a pass against the run is struck only after braking and turning (body within the set-up angle); normal passes are controlled > 95 %, driven balls are hard; a heavy touch from a 20 m/s ball stays within a few metres |
| `tests/movement.rs` | off-ball players in live play average < 3.9 m/s (3.75 since phase D; the history is in the file), walk > 28 % of the time and turn < 60°/s (≈ 4.1 m/s, 20 %, 82°/s without the movement mechanisms) |
| `tests/personality.rs` | in a 2 v 1 near the box: egotism raises the share of self-actions; a humble player with perfect instinct plays the optimal pass; the held decision noise reproduces the softmax over many seeds, keeps the choice when re-deciding a moment later, and is drawn afresh after `decision_noise_hold_s` |
| `tests/keeper.rs` | with error-free shots: a shot from 25 m to 2 m beside a keeper on his GK-01 spot is saved ≥ 102 / 120 times (≈ 90 %; enough trials that the random stream doesn't decide it) (dive planned with the time left, only as hard as needed; 32 / 40 with the old full-stretch dive); a 9 m shot 0.4 m inside the post beats him ≥ 30 / 40 times |
| `tests/blocks.rs` | an error-free low shot passing 0.2 m from an outfield defender 3 m in front of the shooter is blocked ≥ 80 % of the time, one passing 1.3 m away (a stretched leg) 25–70 %, one 2.4 m away never |
| `tests/xg.rs` | long shots (25–35 m) against a keeper on his GK-01 spot get xG < 0.04 with no keeper bonus; a keeper caught 14 m out adds > 0.1; a penalty-spot shot against a set keeper is 0.12–0.4; a weak-foot shot gets 0.7–0.95 of the strong-foot xG; a shot 12 m wide gets < 0.75 of a central one at the same distance |
| `tests/instructions.rs` | a team's defending instructions change its layer-2 block and leave its layer-1 block and the other team untouched; an obedient full-back (discipline 20, attunement 1) takes up the full instructed width, a wilful one (discipline 1, attunement 0) only part; the BOX-01 first defender contains at the instructed distance. Unit tests in `team::tactics`: defaults equal the catalog values, an explicit line beats the block height, players apply instructions as far as they adhere |
| `tests/brains.rs` | with exact perception every player of a side holds the same beliefs after every brain update, and they are really evaluated (duties assigned); in every formation the rest defence comes from the roles with the rest-defence duty; test 10 (individual brains, out of possession, 10 min of a match): exactly one first defender (two in the counter-press), a line leader with the line-leading duty, no opponent marked or tracked twice; in possession (phase E): 4–5 in the rest defence always, one depth claimed in > 75 % of advanced ticks, two offers within 6 m < 5 %, duplicate runs < 0.5 %; test 13 a wing-back plays higher and stays back less than a full-back |
| `tests/match_sanity.rs` | same seed → same match, different seed → different match; a full 90 minutes stays finite and bounded, both teams shoot and pass; every shipped formation plays a sane match |

## Calibration status (40 matches, default squads)

A real 90-minute match has only ≈ 57 minutes of ball in play: 54:49 in the Premier League 2022/23 and 58:11 in
2023/24 ([The Analyst](https://theanalyst.com/articles/premier-league-ball-in-play-are-we-seeing-less-football-2025-26)).
The engine simulates open play without fouls, injuries or substitutions and restarts within seconds, so the ball is
in play for ≈ 84 of its 90 minutes. Real per-match counts (goals, shots, take-ons, tackles) are therefore scaled by
84.4 / 57 ≈ 1.48 to compare with the engine; the lab does this with the measured in-play time. Rates and ratios
(conversion, save %, goals per xG, pass completion, shares) need no scaling.

Not every row is a target. Real matches differ by style, so volume and mix rows (goals, shots, passes, long and
lofted balls, tackles, possession changes) are plausibility checks against the real spread, measured per style
archetype on the `styles` bench. Only situation-conditional rows (outcomes in a given geometry, checked as
style-invariant in StatsBomb) are calibrated against. "Short-pass losses and conditional targets" below lists which
row is which.

| Metric | Engine | Target | Status |
|---|---|---|---|
| goals per match | 2.95 from 3.03 xG (2.17 from 2.61 before the final-third round; 3.03 before the shot-selection round, its xG unchanged; 2.50 before the carry-in-space round, 3.00 before the pressed-receptions round, 3.05 before the short-pass round, 2.85 before the lofted-volume round, 3.33 before the headed-pass-error round, 2.65 before the one-step-valuation round, 2.53 before the direct-play round, 3.30 before the attribute-slope round, 3.83 before the pass race and IND-08, 3.75 before the contested high balls, 3.62 before the settle time, 3.83 before the long-ball choice, 3.55 before the sourced block shapes, 3.03 before the lofted-pass fix, 4.50 before ATT-25; 40-match noise ≈ 0.3–0.5) | 1.5–4 per real match → 2.2–5.9 (real ≈ 2.8 → ≈ 4.2) | low: the settled attack is weak (crosses are M4) |
| ball in play (lab) | 84.4 of 90 min | by design (open play only) | targets scaled |
| goals per xG (`shots`) | 0.88 / 0.74 in two 40-match runs (lab ≈ 0.83); box 1.17 / 0.81, outside 0.51 / 0.63 | ≈ 1 | noisy; outside low |
| goals per xG, weak / strong foot (`shots`) | 0.53 / 1.09 (73 weak-foot shots in 40 matches; 1.3 / 1.3 in the run before) | equal (xG prices the foot in) | noisy |
| goals per xG, wide / central footed shots (`shots`) | 0.84 / 1.09 | equal | ok |
| shots from 0–11 m (`shots`) | ≈ 5.5 % (≈ 27 % in the M3 box-defending rounds) | ≈ 14 % of StatsBomb footed open-play shots | **low** (see "Carry in space") |
| opponents in the shooting triangle at a shot (`saves`) | 0.13–0.17 | 0.4–0.9 (StatsBomb, by band) | **low** |
| shots per team | ≈ 22 in the lab (≈ 44 per match; ≈ 38 before the final-third round, ≈ 43 before the shot-selection round, ≈ 38 before the carry-in-space round, ≈ 33 after the settle time) | 8–20 → 12–30 (≈ 25 per match → ≈ 37; open play only ≈ 16 → ≈ 24) | **high**: the excess is from outside the box |
| shots from outside the box (`shots`) | ≈ 72.5 % (≈ 73.8 % before the shot-selection round, ≈ 71 % at the start of the carry-in-space round) | ≈ 32 % ([PL 2024/25](https://theanalyst.com/2025/03/premier-league-2024-25-shot-data): 31.7 %); 46 % of footed open-play shots in [StatsBomb open data](https://github.com/statsbomb/open-data) | **high** (see "Shot selection and hold pricing") |
| shot share of passes + shots at 0–11 / 11–16.5 / 16.5–22 / 22–30 / 30–40 m from goal (`shot_share`) | 78 / 75 / 52 / 27 / 4.1 % (95 / 75 / 62 / 28 / 4.3 % before the final-third round, 93 / 75 / 55 / 32 / 4.4 % before the shot-selection round; the 0–11 and 16.5–22 m bands have ≈ 30 / ≈ 110 shots per run) | 85.6 / 45.7 / 23.9 / 7.9 / 0.2 % (StatsBomb footed open play; conditional) | **high** outside the box (see "Shot selection and hold pricing") |
| shot share per player at 16.5–30 m, ≥ 25 actions, median / max (`shot_share`) | 33 / 53 % (the five attackers) | 14 / 24 % (StatsBomb, 32 players) | level **high**, spread ok |
| unblocked shots on target (`saves`) | ≈ 58 % (0–11 m 73–80 %) | ≈ 50 % ([PL 2022/23](https://www.premierleague.com/news/2910118): best team 55.4 %, 8 of 20 teams below 50 %) | ok |
| save % of shots on target (`saves`) | ≈ 60 %; by band 0–6 / 6–11 / 11–16.5 / 16.5–22 / 22–30 m: 20 / 46 / 68 / 87 / 89 % | keepers ≥ 500 min 51.5–73.5 % ([PL 2024/25](https://theanalyst.com/2025/04/premier-league-goalkeeper-stats-2024-25)); StatsBomb footed open play by band 23 / 50 / 66 / 82.5 / 90 / 96 % | ok |
| keeper depth at the shot, mean by band (`shots`) | 2.1 / 2.1 / 3.1 / 3.3 / 3.4 / 5.9 m | StatsBomb 1.5 / 1.8 / 2.2 / 2.2 / 2.1 / 1.8 m | close; high after turnovers |
| conversion inside / outside the box (`shots`) | ≈ 12–16 % / ≈ 1.8–2.5 % | 14.7 % / 4.2 % ([PL 2024/25](https://theanalyst.com/2025/03/premier-league-2024-25-shot-data)); StatsBomb footed 14.0 % / 3.1 % | outside low |
| passes / shots with the weak foot (`shots`) | 14 % / 7 % | one-footed players in StatsBomb: 13.1 % / 21.7 % | shots low |
| shots blocked by an outfield player (`saves`) | ≈ 22 % (17–26 % by band) | ≈ 25 %; StatsBomb footed open play 32 % (12–38 % by band) | low at range (few defenders in the lane) |
| successful take-ons per team / success rate (`duels`) | ≈ 20 / ≈ 54 % (≈ 37 before the pressed-receptions round) | ≈ 8–12 → 12–18 / ≈ 50 % | slightly high |
| tackles / passes first touched by an opponent per match | ≈ 188 (≈ 108 won; ≈ 248 before the carry-in-space round, ≈ 355 before the pressed-receptions round) / ≈ 247 (≈ 122 before the pass race and IND-08) | StatsBomb ×1.48 per match: ≈ 50 tackles, ≈ 61 balls lost by a tackle or dispossession; ≈ 250 open-play passes not completed | tackles **too many**; lost passes ok (see "Pass risk, long balls and transitions") |
| pass completion | 82 % (79–80 % before the short-pass round, 87–88 % before the pass race) | 65–90 %; StatsBomb open play 82 % (style-dependent) | plausible; the conditional target is completion by pressure on the passer (next row) |
| ground 5–40 m completion by the nearest opponent to the passer 0–1.5 / 1.5–3 / 3–5 / 5–8 / 8+ m; first-time share there (`intercepts`) | 84 / 86 / 87 / 91 / 96 %; 33 / 25 / 21 / 8 / 1 % | 88 / 92 / 95 / 97 / 99 %; 7 / 8 / 10 / 7 / 4 % (StatsBomb 360, style-invariant within 2–3 points) | **low**: first-time escapes from pressure 3–5× real (see "Short-pass losses and conditional targets") |
| open-play passes per team | ≈ 650–700; 30–45 m ≈ 31 % (`passes`) | ≈ 710; 30–45 m 7.8 %, switches of play ≈ 98 per match | **long**: 3–4× too many long balls and switches (see "Long-ball choice") |
| lofted 30–45 m passes (`passes`, `high_balls`) | 100 per match, 68 % complete, kick to first contact 2.52 s, aerial duel 28 % | StatsBomb ≈ 56, 48 %, 2.45–2.56 s, 14–15 % | **high**: chosen ≈ 1.8× as often (see "Lofted volume") |
| longest live ball standstill (lab) | 7.4–10.8 s | < 30 s (stuck-state guard) | ok |
| receiver loses the ball before his next pass or shot (`settle`) | 8 % | 5.9 % (StatsBomb) | ok |
| receipt → next pass or shot (`settle`): first time / 0.1–0.5 s / median / > 1 s | 21 % / 13 % / 0.72 s / 40 % (set T, 2026-11-02; was 31 / 10 / 0.58 / 24) | 18 % / 7 % / 1.22 s / 59 % (StatsBomb; shots alone: 47 % first time) | quick: free carriers still release without carrying (see "Set T") |
| ground-pass receptions by the nearest opponent at the receipt 0–1.5 / 1.5–3 / 3–5 / 5–8 / 8+ m (`receptions`): first time; lost before the next pass or shot, of all / of the controlled ones | 69 / 63 / 56 / 27 / 19 %; 9.8 / 8.6 / 5.9 / 4.1 / 1.2 %; 32 / 23 / 13 / 5.6 / 1.5 % (a heavy touch counts as lost only when the opponents win the ball) | 24.5 / 24.7 / 16 / 6.5 / 2.2 %; 20 / 11 / 6.3 / 2.6 / 0.8 % (plus fouls won 7.1 / 3.5 / 1.8 / 0.9 / 0.2 %); 27 / 15 / 7.5 / 2.7 / 0.8 % (StatsBomb 360, style-invariant) | **first time 3–9×; losses in space 2–2.5×** (see "Carry in space") |
| nearest opponent to the passer at a pass (`support`) | 4.2 m | 2.9 m (StatsBomb 360) | far |
| support at a pass: nearest team-mate, team-mates within 10 m (`support`) | 8.7 m, 1.02 | 8.5 m, 1.09 (StatsBomb 360) | ok |
| deepest outfield defender at a central open-play pass, ball 18–22 / 22–26 / 26–30 m from the goal line: drop share of the ball's distance, passer free / an opponent within 3 m (`final_third`) | 0.39 / 0.45 / 0.40; 0.36 / 0.37 / 0.35 (0.30 / 0.33 / 0.32; 0.31 / 0.32 / 0.29 before the final-third round) | 0.39 / 0.41 / 0.43; 0.33 / 0.35 / 0.38 (StatsBomb 360; conditional) | ok |
| most advanced team-mate behind the deepest defender at an open-play pass, ball 18–50 m from the goal line (`final_third`) | 5–7 m at every distance | 1.0–1.5 m at every distance (StatsBomb 360; conditional) | **far** (see "Final-third support and line depth") |
| team-mates in the box at a central open-play pass, ball 18–22 / 22–26 / 26–30 m, free / pressed (`final_third`) | 0.4 / 0.3 / 0.2; 0.3 / 0.2 / 0.1 | 1.7 / 1.2 / 0.7; 1.4 / 0.7 / 0.4 (StatsBomb 360) | **low** (follows the row above) |
| nobody within 2 m of the line (deepest outfield opponent) at an open-play pass, ball 10–50 m, passer free / pressed (`forward_moves`) | positions 77 / 85 %; targets 39 / 51 % | 33 / 38 % (StatsBomb 360; positions; conditional) | targets close, **positions far** (movement lag) |
| centre-forwards' forward receptions in the attacking half on the line / short / dropped (g < 2 / 2–8 / ≥ 8 m); line 0–15 m from goal (`forward_moves`) | 2 / 43 / 56 %; 5 / 56 / 40 % | 21 / 46 / 33 %; 42 / 42 / 15 % (StatsBomb 360; conditional) | **few on the line** |
| striker profile spread, `PROFILE=runner` / `link`: targets on the line, receptions dropped (`forward_moves`) | 60 / 19 %; 48 / 62 % | per CF dropped p10 15 / p50 32 / p90 51 % (StatsBomb 360) | direction ok, level high |
| headless match time | ≈ 6.5 s (one core; 7.1 s at the start of the carry-in-space round) | < 5 s | **slow** (grew in earlier rounds) |
| changes of possession (`transitions`) | ≈ 275 per match (in regular play 219, from throw-ins 34; 283 before the settle time, 224 before the sourced ATT-11 shape) | ≈ 162 (StatsBomb ×1.48; 88.5, 20.8) | **too many** |
| mid-block length / width / line from own goal | 30.3 m / 38.8 m / 30.9 m | ≈ 27.2 m / ≈ 40.1 m / 37.2–37.9 m | line **deep** |
| low block length × width / line | 19.3 × 30.4 m / 15.4 m (12.4 m before the final-third round) | 24.7–25.5 × 35.5–36.0 m / 17.6–18.1 m | **short, narrow, deep** |
| in possession, build-up / progression / final third: length × width, line | 34.6 × 42.0 / 32.1 × 48.7 / 36.7 × 49.4 m; line 19.2 / 34.0 / 44.7 m | 39.2 × 54.0 / 33.0 × 55.3 / 36.2 × 44.3 m; line 19.6 / 40.3 / 53.3 m | build-up narrow, lines deep |
| GK distance to goal line / to defensive line | 11.5 m / 19.9 m | ≈ 13.1 m / ≈ 20.4 m | close |
| pressers 2–3 | 42 % | ≈ 58 % | low |
| counter-press duration | 4.4 s | window ≈ 5 s | ok |
| one-touch goals | 9 % | 63–80 % | far off (crosses / cut-backs are M4) |
| goals from 1–5 passes | 44 % | ≈ 53 % | close |
| counter-attacks | 8.4 s / 36 m | 9–11 s / ≈ 70 m | short |
| off-ball speed in live play / walking share | 3.49–3.54 m/s over 3 seeds, 29–30 % walking (`movement`, final-third round; 3.1–3.35 m/s / 37 % in earlier rounds, 3.0–3.3 before the sourced ATT-11 shape) | ≈ 2–2.5 m/s (10–11 km per match) | high |
| kicks struck > 90° from the body (not first time) | 2–5 % | rare | ok |
| heavy first touches | 3.5 % of receptions (≈ 25 per match), 78 % within 1 m | rare | ok |

Dribbling fix (`turnovers`, `dribbles` examples): carriers used to lose their own dribble on every sharp turn (the
touch was played at full carry speed whatever direction they were running), and a defender who chose not to tackle
still won a dribbled ball by touching it. Touches are now weighted to the carrier's velocity along the new direction
(a cut or drag-back on a turn), and a controlled ball can only be won in a tackle duel (IND-03). This cut open-play
turnovers from ≈ 690 to ≈ 415 per match.

Movement, set-up and touch fixes (`movement`, `kicks`, `touches` examples), after watching a match: off-ball players
ran at ≈ 4.1 m/s and changed direction at 82°/s because every 0.36 s decision re-picked a point a few metres away and
ran to it at near full urgency; players hit long passes straight out of a dribble in the opposite direction; and one
reception in six was a heavy touch that sent the ball 3–10 m. See "Body, movement and touch" above. Open-play turnovers
fell further to ≈ 287 per match. (The `movement` example accumulates in f64: f32 sums of 0.02 s ticks over a match
were off by up to 20 %.)

xG and decision-noise fixes (`shots` example, which breaks xG into its factors per distance band). The old
keeper term rated 30–40 m shots at ≈ 0.12. The race model (see "Three layers") gives ≈ 0.01–0.03 against a set
keeper, and the per-band mean xG now matches what the engine converts from distance (22–40 m: ≈ 0.05–0.07 mean xG
against 4–8 %). The xG fix alone hardly reduced long shots, because they came from the softmax being re-drawn every
0.3 s.

40-match labs, one change at a time:

| Change | Goals per match | Shots per match | Passes per match |
|---|---|---|---|
| Before (20 matches) | 13.1 | 58 | 928 |
| xG fix | 11.8 | 66 | 932 |
| xG fix + shot variants share one weight | 12.3 | 54 | 943 |
| xG fix + held decision noise | 14.2 | 70 | 847 |
| All three | 17.0 | 60 | 847 |

With held noise the carriers keep to their decision instead of flip-flopping, attacks reach the box more often, and
the weak box defending shows. Setting `decision_noise_hold_s` near 0 restores a fresh draw at every decision.

Box defending (`box` example: defenders at the moment of a shot per distance band, shot outcomes, what the nearest
defender was doing; `spell` example: one carrier's spell second by second). Box shots almost never followed a pass:
the shooter had the ball for 6–9 s, dribbling in front of four or five defenders, because IND-03's contain never
ended against a better dribbler, nobody covered or blocked the shooting line, and only 8–10 % of shots were blocked.
BOX-01..03 (catalog) make the first defender tight and show the carrier outside, put the cover on the shooting line,
and let a covered defender engage; pursuit and shot blocks are body mechanics (above).

| Change (40 matches) | Goals per match | Shots per match |
|---|---|---|
| Before (xG + held noise) | 17.0 | 60 |
| BOX-01 / BOX-02 | 14.9 | 53 |
| + shot blocks + pursuit | 11.3 | 48 |
| + BOX-03 | 11.3 | 46 |

Shots from 0–11 m fell from ≈ 14.8 to ≈ 10.1 per match, and from 11–16.5 m from ≈ 14.3 to ≈ 10.8.

After box defending, the shots that were still taken were as good as before: the engine converted ≈ 43 % from 6–11 m
and ≈ 30 % from 11–16.5 m (mean xG ≈ 0.25 and 0.17; real ≈ 12 % for the box). At the shot the nearest defender was
usually one who had just been beaten (≈ 40 % "recover"). The spells ended with one successful take-on, after which
the carrier ran into the box at 7–8 m/s with every defender behind him.

1 v 1 duels (`duels` example: every pair of a carrier and the first defender set in front of him, ≥ 1 m ahead on his
line to the goal centre, ≤ 1.5 m to the side, within 3 m, followed until the defender wins the ball, the carrier
releases or loses it, the defender is beaten in or without a tackle, drops off, or after 3 s still in front, "held
up"). A beaten defender's role went to whoever was quickest to the contain point as if it stood still, so the chaser
kept it and the defender in the carrier's path waited; the cover did not move with the carrier and was level with
him when the first defender was beaten; the carrier could sprint to every knock-on at full top speed. Fixes: DEF-03
time to meet the moving contain point and an engaged first defender who keeps the role until the ball is past him,
DEF-02 cover that stays goal-side and moves with the carrier (catalog), tracking and the carrying speed (above).

| Change (40 matches; `duels`, 6 matches) | Goals per match | Shots per match | Successful take-ons per team / success |
|---|---|---|---|
| Before (box defending) | 11.3 | 46 | — |
| IND-01 tracking + DEF-03 time to meet | 9.9 | 47 | ≈ 45 / 69 % |
| + engaged first defender keeps the role | 9.9 | 47 | ≈ 38 / 62 % |
| + DEF-02 cover moves with the carrier | 9.8 | 46 | ≈ 44 / 65 % |
| + carrying speed (catch-up `+ 0.25` → `+ 0.1`) | 10.5 | 45 | ≈ 37 / 56 % |

Over 40 matches the goals per match have a standard error of ≈ 0.5, so the last three rows do not differ. Close
shots (0–11 m) fell from ≈ 9.0 to ≈ 6.4 per match. Tried and dropped: challenging whenever the carrier comes level to
go past (BDFL-KOHF: "schiebt in dem Moment, in dem der Angreifer an ihm seitlich vorbei will, den Körper zwischen Ball
und Gegner") as a tackle at the duel model's odds. It raised successful take-ons to ≈ 44 per team, because it turned
hold-ups into coin flips. The source describes a body-contact block, not a tackle, and that needs its own body
mechanics.

Shot accuracy and saves (`saves` example: every shot per distance band followed to its outcome, i.e. blocked,
saved, goal after a failed save roll, goal out of the keeper's reach, or off target; plus the predicted miss (wide or
over), where on the goal it crossed, the keeper's dive and save probability). The engine converted about 1.6 times
its own xG. Box shots were ≈ 74 % on target and only ≈ 47 % of those were saved. Most box goals came after the
keeper had reached the ball and lost the save roll, because his dive always counted as full stretch (see
"Goalkeeper dive" above). Misses went wide about ten times as often as over the bar.

| Change (`saves`, 20–40 matches) | Shots on target (of unblocked) | Save % | Goals per shot |
|---|---|---|---|
| Before | 51 % (≈ 58 %) | 57 % | 22 % |
| Keeper dive planned with the time left and only as hard as needed | 51 % | 72 % | 14 % |
| + shot speed–accuracy (`shot_speed_exp` 2) | 49 % | 72 % | 13.5 % |
| + shot error `angle_shot` 0.10 → 0.12, `elevation_shot` 0.06 → 0.09 | 43 % | 76 % | 10.4 % |
| + `save_bias` 1.0 → 0.5 (76 % is above the best 2024/25 PL keeper's 73.5 %) | 42 % (48 %) | 71 % | 12.4 % |

The 40-match lab went from 10.5 goals per match (6.6 xG) to 6.5 goals (7.1 xG). Larger shot errors hardly change
close-range accuracy (0–11 m stays ≈ 60–75 % on target, because the goal is wide from there), and over the bar is now
≈ 15–20 % of misses. Per band the engine now converts about its own xG from 6 to 16.5 m (26 % vs 0.24, 16 % vs
0.18). Beyond the box xG overrates its shots (16.5–40 m: mean xG 0.08–0.12 against 3–7 % converted), which makes long
shots look better to the on-ball AI than they are. That fits the ≈ 60 % of shots from outside the box. The remaining
goal excess is volume and chance quality: ≈ 46 shots per match instead of ≈ 25, box shots with a mean xG of ≈ 0.23,
and only ≈ 12 % of shots blocked. In the duels, about half of the "beaten without a tackle" pairs passed within the
defender's tackle reach while he held back (IND-03 needs p ≥ 0.65), and carriers still take on set defenders far more
often than real ones (≈ 67 attempts per team). Tackles (≈ 220) and interceptions (≈ 142) per match are several times
real values. The diagnostic examples (`shots`, `saves`, `goals`, `defending`, `finishing`, `box`, `spell`, `duels`, `snapshot`, `tackles`,
`turnovers`, `dribbles`, `block`, `keeper`, `movement`, `kicks`, `touches`) isolate these effects.

xG recalibration with the foot and the position (see "xG" and "Feet" above). The old base curve came from published
curves for central shots and the keeper term could lift a 30–40 m shot to ≈ 0.07–0.25, so long shots looked
better to the on-ball AI than they were. Steps, on the `shots` example (`SOFT`, `HARD`, `SLACK` env overrides for
the sweeps) and the `finishing` bench:

1. Base curve refitted to StatsBomb, with the lateral term.
2. Finishing factor as a power of the error-scale ratio, which takes in the foot, first-time and body angle (it was
   0.7 + 0.6·skill).
3. Foot geometry reversed (each foot kicks across the body).
4. The open-goal value falls with the goal angle.
5. Passes set up on the strong foot.

With a fixed 30° limit to the foot's own side, only 2 % of footed shots were weak-foot ones and 26 % of passes. A
speed-dependent limit (60° soft → 0° at full power) gave 10 % of shots. The strong-foot pass preference then brought
passes to 14 %. Keepers in the engine then stood 7–9 m off their line at 22–40 m shots, where real ones are
mostly < 4 m off (fixed in the next step). The keeper term therefore stays, but with the steeper open-goal value the
shots it lifts score about their xG.

| 40 matches | Before | After |
|---|---|---|
| shots per match | 46.5 | 31.7 |
| shots from outside the box | ≈ 60 % | ≈ 45 % |
| goals / xG per match (lab) | 6.5 / 7.1 | 6.1 / 4.5 |
| mean xG and conversion, 22–30 m / 30–40 m | 0.08–0.12 vs 3–7 % | 0.054 vs 5.7 % / 0.053 vs 5.3 % |
| goals per xG outside / inside the box | ≈ 0.6 / ≈ 1.0 | 0.9 / 1.3 |
| weak-foot shots / passes | — | 10 % / 14 % |

The xG now matches real data, and that exposes how much easier finishing is in the engine than in reality. On the
`finishing` bench (placed far-corner shots against a set keeper, skill 11), the engine converts 53 % from 8 m
central, where the real clean-shot curve gives ≈ 0.39. At range the bench converts less than the real curve (8.6 %
vs 15.6 % at 16 m, 1.1 % vs 3.1 % at 30 m), because it is on target 50–58 % (real 32–38 %) but saved 93–98 % (real
86–91 %). In matches the 0–11 m bands score 1.35–1.5 times their xG, at save % 11 (0–6 m) and 48 (6–11 m). Close-in
saving is the next lever on goals per match. Weak-foot shots stay at 10 % against a real 21.7 %. First-time shots are
rare in the engine (1–13 % by band), and in real play the weak foot is mostly used when the ball arrives on that
side.

Keeper position, saves and blocks, all measured against StatsBomb freeze frames (footed open-play shots, the same
610 matches as the xG fit). `saves` prints each shot's outcome by band, the save % of shots on target, and the
blocked share by opponents in the shooter–posts triangle; `GK_NARROW`, `GK_PER_M`, `GK_MAX`, `GK_FRAC`,
`SAVE_BIAS`, `SAVE_SLOPE`, `REACH`, `LEG_P` and `LEG_H` override the parameters for sweeps. `goals` classifies every
goal by the event before it. Steps:

1. **Keeper depth (GK-01).** Real keepers stand 1.2–2.3 m off their line at the shot (median by band), and > 6 m in
   about 1 % of 20–35 m shots. The engine's GK-01 depth was 3 m + 0.12 m per metre beyond 11 m, so 6–9 m at long
   range. It is now `gk01_depth`: 1.8 m + 0.12 m per metre, at most 2.5 m and 0.3 of the ball distance. GK-03 and
   BU-01 blend into it within 30–40 m of goal. Before, BU-01 pushed the keeper up as a passing option whenever his
   own team had the ball, even inside shooting range. 45 % of long shots came within 5 s of a regain, with the
   keeper caught high in 84 % of them. Real teams take 11 % of their 16.5–40 m shots that soon after a regain, with
   the keeper 2.2 m out on average. In the engine the keeper is now caught high (> 3 m above his GK-01 depth) in
   35 % of those shots and 1 % of the rest.
2. **xG base with a first-time term** (see "xG" above). Controlled shots convert 5–10 % better in the box than the
   old mixed curve said.
3. **Save bias.** `save_bias` 0.5 → −0.15 is the best fit to the real save % by band (sweep 0.5 / 0 / −0.3 / −0.6
   over 60 matches). With the old keeper depth, 0.5 had stood in for the keeper being too far out.
4. **Shot blocks.** A stretched leg or a slide now reaches a low ball within 1.4 m, and an opponent meets the shot
   at its closest approach (see "Shot blocks" above). Reach 0.75 / 1.4 / 1.8 / 2.2 m gave 14 / 22 / 27 / 31 %
   blocked. With 1.4 m, the blocked share with one opponent in the triangle is 36 / 53 / 64 / 48 / 37 % from 0–6 to
   22–30 m (real 23 / 42 / 51 / 48 / 39 %). With none in it, it is 14–24 % (real 3.5–23 %), because defenders
   outside the triangle still reach the ball and some step into the lane after the shot.

| 40 matches (lab) | Goals | xG | Shots | Save % (`saves`, 0–6 / 6–11 / 11–16.5 / 16.5–22 / 22–30 m) | Blocked |
|---|---|---|---|---|---|
| Before | 6.07 | 4.52 | 31.7 | 11 / 48 / 67 / 79–86 | 11 % |
| Keeper held in goal in possession (BU-01 / GK-03) | 6.40 | 4.65 | — | — | — |
| + GK-01 depth | 5.35 | 4.29 | — | — | — |
| + xG with the first-time term | 5.80 | 4.69 | — | 25 / 57 / 79 / 91 / 93 (60 matches) | 11 % |
| + `save_bias` −0.15 | 7.62 | 4.91 | — | ≈ 14 / 45 / 68 / 85 / 90 (between the 0 and −0.3 sweeps) | 11 % |
| + leg blocks, met at closest approach (final) | 5.93 | 5.06 | 29.1 | 19 / 52 / 70 / 80 / 84 | 22 % |
| Real (StatsBomb, ×1.48 for time in play) | ≈ 4.2 | — | ≈ 37 | 23 / 50 / 66 / 82.5 / 90 | 32 % |

Keepers now stand and save like real ones by band, and the `finishing` bench (keeper set, skill 11) converts about
the real curve from 12 m (0.27 vs 0.27). From 8 m it converts 0.60 against the curve's 0.43: a placed far-corner shot
from there beats the keeper 91 % of the time (`keeper` bench). From 16 m and 30 m it converts less than the curve
(0.10 vs 0.17, 0.013 vs 0.032). In matches, goals per xG is ≈ 1.03 for shots (box 1.07, outside 0.74). The lab's
1.17 includes own goals (≈ 0.23 per match against ≈ 0.14 real) and deflected or rebounded goals (`goals`: 86 % of
goals come straight from the scorer's shot). Goals per match are still ≈ 1.4 times real. Most of the excess is chance
quality, not finishing or saving:
- 34 % of shots come from 0–11 m (real ≈ 14 %), mostly controlled and unpressured: 1–3 % of them are first time
  (real 70–83 %).
- Opponents in the shooting triangle at a shot average 0.1–0.2 (real 0.4–0.9). For a carrier in range they average
  0.47 close in down to 0.18 at range.

The open levers are box defending (lane occupancy, pressure on the carrier in the box), first-time finishing from
crosses and cut-backs (M4), the 0–6 m accuracy (79–82 % on target unblocked vs real 63–66 %), and own goals.

How the defence lets shots happen (`defending` example, 40 matches, against 9,919 StatsBomb open-play shots without
free-kick or corner patterns; `DUMP=file.jsonl` writes the engine shots in the same layout). "Settled" means the
possession lasted ≥ 10 s and began > 40 m from goal; everything else is a transition. Real per-match counts × 1.48.

| At the shot | Real | Engine |
|---|---|---|
| shots from 0–11 m per match, settled / transition | 4.4 / 2.0 | 4.6 / 6.2 |
| possession began ≤ 40 m from goal; possession < 5 s | 22–26 %; 4–7 % | 50–62 %; 25–42 % |
| assisted (a pass to the shooter), 0–11 m / 16.5–22 m | 73–82 % / 78 % | 5–7 % / 23 % |
| shooter's carry in his spell (median), 0–11 m / 16.5–22 m | 0 m / 7 m | 19–40 m / 27 m |
| passes after the ball came within 30 m of goal (mean) | 3.2–4.2 | 0.2–0.4 |
| shooter's team-mates in the box | 2.0–2.6 | 0.4–0.8 |
| settled, shot from 22–30 m: defenders between ball and goal line / in the box / in the shooter–posts triangle | 6.2 / 3.2 / 0.76 | 4.7 / 1.3 / 0.23 |
| settled, shot from 22–30 m: deepest four defenders, metres from the goal line; their width | 13.7–16.5; 18.6 m | 17.1–20.5; 25.9 m |
| settled, shot from 16.5–22 m: the same | 10.2–13.7; 15.8 m | 11.8–16.5; 24.3 m |
| transition, shot from 22–30 m: the same | 14.0–17.0; 18.7 m | 16.5–22.4; 28.7 m |

Real defences drop their back four into the box and narrow it to 15–19 m when the ball is in shooting range. The
engine's block (`defensive_block`) keeps the DEF-05 width of 40.1 m everywhere, with the full-backs anchored at
± 0.85 of it, and the line follows the ball at most `def05_shift_speed` (3 m/s), slower than a carrier running at it.
Engine shots come mostly from regains near the opponent's goal and long solo carries with no pass after entering
the final 30 m. That is partly defending (no one stops the carrier; tactical fouls came with M7, TRN-04) and partly
the attack (dribbling instead of passing, nobody joining in the box, no crosses until M4).

### Defending the box (BOX-04)

BOX-04 (sources [FIFA-LB](https://fifatrainingcentre.com/en/practice/elite-sessions/out-of-possession/collective-ball-recovery-in-a-low-block.php)
and [FIFA-B4](https://www.fifatrainingcentre.com/en/practice/elite-sessions/out-of-possession/defending-with-a-back-four.php))
reshapes the block with the ball within 45 m of the own goal centre, fully at 22 m (`brain::box04_block`):
- The back line narrows until neighbouring defenders are at most 6 m apart.
- The block shifts less to the ball side, so the far full-back tucks in beside the centre-backs.
- The block shortens until the midfield line is 6 m in front of the back line.
- Its lines drop as fast as the ball comes towards goal, not at the 3 m/s DEF-05 cap.
- A back-line player whose point drops runs there at urgency ≥ 0.7 instead of settling.

Before, the back four stood 22–33 m wide with the ball central 11–30 m out. The narrowing alone brought the width to
the real 15–19 m and goals to 5.43. Shortening the block put more defenders between the ball and the goal.

| At the shot (40 matches; real StatsBomb, counts × 1.48) | Real | Before | BOX-04 |
|---|---|---|---|
| settled, 11–16.5 m: deepest four defenders, metres from the goal line; their width | 7.2–11.0; 14.5 | 9.2–12.9; 20.9 | 7.1–10.4; 12.7 |
| settled, 16.5–22 m: the same | 10.2–13.7; 15.8 | 11.8–16.5; 24.3 | 11.2–15.3; 16.3 |
| settled, 22–30 m: the same | 13.7–16.5; 18.6 | 17.1–20.5; 25.9 | 13.4–18.4; 19.9 |
| settled, 22–30 m: defenders between ball and goal line / in the box / in the triangle | 6.2 / 3.2 / 0.76 | 4.7 / 1.3 / 0.23 | 5.6 / 2.3 / 0.27 |
| transition, 16.5–22 m: deepest four; width | 10.5–13.7; 15.9 | — | 12.0–17.7; 18.7 |
| shots per match, settled / transition | 23.4 / 12.1 | 12.3 / 16.9 | 10.1 / 15.9 |
| shots from 0–11 m per match, settled / transition | 4.4 / 2.0 | 4.6 / 6.2 | 2.2 / 5.7 |
| lab goals / xG / shots | ≈ 4.2 / — / — | 5.93 / 5.06 / 29.1 | 4.85 / 4.22 / 26.1 |
| blocked (`saves`) | 32 % | 22 % | 28 % |

A settled defence now looks like a real one when the shot comes. Engine attacks against it produce fewer than half
the real number of shots. Transitions produce a third more than real, and nearly three times as many from 0–11 m.
In those the back four is still 1.5–4.5 m high and 1.5–3.5 m wide because it is running back.

What remains is mostly outside the box:
- 73 % of shot possessions begin with an interception or tackle, on average 31–41 m from the goal. Real: 35 % from
  tackles, recoveries and interceptions, and 40 % from throw-ins, goal kicks and the keeper.
- Few defenders stand in the shooting triangle (0.13–0.27 vs 0.33–0.76 in settled play). Engine shooters wait for a
  clear lane; real ones shoot through bodies.

Two notes:
- Real per-match counts in the earlier diagnosis table were not scaled for the 0–11 m row; corrected above (4.4 / 2.0).
- The lab's mid-block length moves between 29.6 and 33.7 m from run to run (target 27.2). The swing comes from
  "organised" samples with the ball in the opponent's third, where the forwards press the keeper while the line holds;
  the metric's `n_mid + n_def ≥ 8` condition counts those as mid-block. Samples with the ball in the own half are
  unchanged at ≈ 24 m.

### Pass risk, long balls and transitions (2026-10-28)

Transition defending was the next topic, so the first step was to compare transitions with StatsBomb (166 matches;
`transitions` example + `trans_compare.py` / `trans_first.py`, real counts ×1.48). The engine has
304 possession changes per match against 162. Engine regains start mostly with a tackle (94.5) or a pass
intercepted (67.5), real ones with a recovered loose ball or a pass (35 + 36). Engine teams regain the ball within 30 m of the opponent's goal
19.4 times per match (real 1.9) and lose it back within 5 s 21–44 % of the time (real 2–6 %). Losers goal-side, the
counter-press window and recovery are close to real, so most of the gap is on the ball, not in transition
defending.

Open-play regains 70–120 m from the goal the winners attack (deep regains):

| | StatsBomb | engine |
|---|---|---|
| per match | 57.9 | 102.4 |
| time to the first action, median | 1.0 s | 1.4 s |
| first pass / clearance length, median | 13.5 m | 22.2 m (28.5 m before the pass-weight fix) |
| first pass ≥ 30 m | 10 % | 37 % |
| first pass under pressure | 18 % | 59 % |
| lost back within 5 s | 2 % | 21 % |

What was checked, and what was found (`pass_speed`, `carry_cal`, `passes`, `settle` examples; StatsBomb scripts
`sb_flight2.py`, `sb_incomplete.py`, `sb_duels.py`):
- **Passes are not lost too often.** StatsBomb teams play ≈ 710 open-play passes per match with 18 % not completed;
  the engine plays ≈ 530 with 14 % not completed. The engine's "interceptions" (≈ 60 per team) count every pass an
  opponent touches first. StatsBomb's Interception event (15.9 per team) is only part of that; the rest are Ball
  Recovery, Clearance, Block and Duel events. The AI's predicted pass success is roughly calibrated (p ≥ 0.95:
  89–91 % reach the receiver).
- **Ball carriers are.** Engine teams win ≈ 69 tackles per match; real teams lose the ball ≈ 30 times by tackle
  (14.4 successful) or dispossession (16.3). Time from receiving a pass to the next pass or shot has a long tail:
  median 0.88 s, mean 2.21 s, p90 6.0 s (real 1.22 / 1.59 / 3.63 s); 12 % are lost first (real 6 %). Each single
  carrying decision is actually kept more often than predicted (dribble 0.96 actual vs 0.89 predicted within 1.2 s),
  but the decision is one step deep and repeats every decision interval. A sideways dribble at keep 0.96 beats a
  recycling pass whose target has a lower possession value, so spells run to 10+ carrying decisions (124 per match,
  21 % end in a lost ball). 65 % of those are wingers in the final third (mean 72 m from their own goal, 58 % of the
  decisions under pressure): with crosses deferred (M4) they have no delivery option there.
- **Long balls.** Lofted 30–45 m passes: ≈ 340 per match against ≈ 41 real high passes of that length. Per type they
  behave realistically: free targets are completed ≈ 91 % (real 85 %). But engine targets are almost always free
  wingers (nearest opponent 8.4 m at the first touch vs 4.2 m), and a third go backward (real 13 %).
- **Low-p passes from decision noise.** 71 ground passes per match were rated p < 0.5 by the AI itself (56 % are
  intercepted). Of those matched to their decision, 44 % were picked at a softmax probability < 0.1. The Gumbel
  noise is held for the whole spell on the ball (`decision_noise_hold_s`), and the carrier re-decides every interval
  until a kick wins, so an option with a high noise draw is eventually played even when it is poor at that moment.

Fixed so far: pass weight (see "Body, movement and touch"), sprint acceleration, aerial duels, the late chaser
(PRS-02) and the block shift during a long ball (BLK-04). Remaining levers, all on the ball: value of keeping the
ball after a dribble under pressure (the one-step myopia), the long-ball choice, wide delivery (M4), and the noise
hold. Transition-defending rules come after these, since their effect cannot be measured while attacks give the
ball back within seconds.

### Receiving and ball security (2026-10-28)

**Receivers (ATT-24).** The `receive` example follows every intended receiver of an open-play ground pass. It
compares completion by the nearest opponent to the end point with StatsBomb 360 (`sb_tight.py`: ground passes of
5–40 m, 8 m around the end point visible). Three faults showed up:
- A receiver who could no longer reach the ball stayed in `Intent::Receive` with a stale point. The team brain does
  not send a chaser after a pass to a receiver, so ≈ 5 % of passes to free receivers rolled out of play. Now
  `off_ball` gives up the receiver when `Simulation::reception` finds no point he can reach. The out-of-play share
  fell to 0.8 %.
- Pressed receivers waited at the aimed point. They now come to meet the ball: during the flight at the earliest
  reachable point of its path (`ai::reach::reception_point`), and already while the passer sets the pass up.
- After that, passes arrived too late in the long bands, so the pass weight was raised (see "Pass weight").

Ground passes of 5–40 m: 94 % completed (StatsBomb 94 %; 85 % before).

| nearest opponent to the end | share, engine / StatsBomb | completed, engine / StatsBomb | AI's predicted p |
|---|---|---|---|
| 0–2 m | 3.1 % / 2.3 % | 30 % / 53 % | 42 % |
| 2–3 m | 2.0 % / 3.6 % | 66 % / 66 % | 54 % |
| 3–5 m | 5.0 % / 14.2 % | 77 % / 86 % | 61 % |
| 5–8 m | 10.4 % / 28.5 % | 88 % / 95 % | 72 % |
| 8 m + | 79.6 % / 51.2 % | 98 % / 99 % | 95 % |

**Carrier (ATT-25).** Every on-ball option is now valued with the pressure on whoever has the ball after it: the
receiver at the target, the carrier at the end of a dribble or at his next decision when holding. This removes the
one-step myopia described above. Its strength was set from StatsBomb receipts (`sb_receipt_press.py`, 60 matches).
Under pressure, the receiver loses the ball before his next pass or shot 21.7 % of the time; free, 4.9 %; overall,
5.9 %. Effect, 10 to 40 matches each:

| | before | after | real |
|---|---|---|---|
| receiver loses the ball before his next pass or shot | 11 % | 6 % | 5.9 % |
| receipt → next pass or shot, p90 | 6.1 s | 5.0 s | 3.6 s |
| spells of 10+ carrying decisions per match (lost) | 134 (23 %) | ≈ 110 (18 %) | |
| changes of possession per match | 315 | 276 | 162 |
| open-play regains whose first action is a won duel | 89 | 58 | 12.6 |
| open-play passes per match (completed) | ≈ 1040 (84 %) | ≈ 1250 (87 %) | ≈ 1420 (82 %) |
| goals per match (lab) | 4.50 | 3.03 | ≈ 4.2 |

80 % of the remaining long carrying spells are wingers in the final third, who have no delivery option until M4.
Goals fell because fewer chances now come from balls given away near goal, which leaves the weak settled attack
exposed (see "Next levers for goals").

**What still makes possession too cheap to lose**, in order of evidence:
- **Long balls go to free targets.** Lofted passes were over-predicted: the receiver got those rated 0.80–0.95
  only 52–66 % of the time. Pricing the kick error into their success and fixing the receiver who ran through the
  drop point made them calibrated (see "Lofted passes"), but did not make them rarer: ≈ 284 lofted 30–45 m passes
  per match (real ≈ 41), because they really do succeed. Their targets are free: nearest opponent at the first touch
  a median 9.0 m (real 4.2 m), aerial duels 1 % (real 11–13 %). Pass completion rose to 91–92 % (real 82 %). The
  block width that left far players free is fixed (see "Block width and in-possession shape"); the long-ball
  preference is not.
- **Contested ground passes are under-predicted** (table above, 3–8 m bands), so the AI avoids them: 20 % of
  engine passes go to a receiver with an opponent within 8 m of the end, against 49 % of real ones.
- **Support is far** (`support` example vs `sb_support.py`, StatsBomb 360, passes with 15 m around the passer
  visible). At a pass, the nearest team-mate is a median 13.5 m away (real 8.5 m), with 0.49 team-mates within 10 m
  (real 1.09) and 1.18 within 15 m (real 2.48). The nearest opponent to the passer is 7.2 m away (real 2.9 m): engine
  passers play long while still free, real ones play short once pressed. A narrower in-possession block shifted to
  the ball side (`attacking_block` half-width × 0.8, `centre_y` 0.4 × ball y) and a smaller ATT-10 crowding radius
  barely changed this (nearest team-mate 12.4 m, pass median 28.5 m). The wide back four and deep keeper circulating
  the ball with 25–48 m passes account for much of the own-third length.
- **No fouls.** Real possessions end with a restart 64–66 % of the time (fouls, ≈ 12 free kicks per match ×1.48),
  engine ones 26–30 %.

### Block width and in-possession shape (2026-10-28)

The `passes` example dumps every open-play pass with the nearest opponent to its end point at release
(`geo_summary.py`, `geo_roles.py`), compared with StatsBomb 360 (`sb_roles.py`, 40 matches). Engine targets were
2.7–3.4 m freer than real ones in every length band. Two causes:
- **Formation slots stopped short of the block edge.** The out-of-possession slots of 4-4-2 and 4-3-3 sat at
  normalised y ±0.85, so the block was 34.5 m wide against `def05_width` 40.1 m. They now reach ±1.0 (in-possession
  slots ±0.95 → ±1.0). Mid-block width 38.1 m.
- **The in-possession shape had no source.** ATT-11 kept the back line a fixed 25 m behind the ball with a constant
  width. It now takes the WC2022 means by third [SPORMETRE-WC22] (see ATT-11 in the catalog), interpolated with
  the ball's x. Line height is a team instruction (`AttackingInstructions.line`).

Nearest opponent to the pass end at release, median:

| length | real | before | after |
|---|---|---|---|
| 0–10 m | 5.6 m | 7.2 m | 7.2 m |
| 10–20 m | 9.6 m | 13.0 m | 11.9 m |
| 20–30 m | 14.4 m | 17.8 m | 15.2 m |
| 30–45 m | 17.9 m | 20.6 m | 18.6 m |

Goals 3.55 → 3.83, completion 91–92 % → 89 %. Changes of possession rose 224 → 270, almost all from the ATT-11
shape (old ATT-11 values with the wider slots: 230), mostly as interceptions (52 → 68 per match): a shorter, narrower
in-possession block puts more passes near opponents, where ground passes are under-predicted (see "Receiving and
ball security"). Long passes did not get rarer: 30–45 m passes are still 41 % of open-play passes (real 7.8 %;
an earlier 3.5 % came from the 360 subset, which under-samples long passes).
Real long passes go mainly centre-back → full-back (12.5 per match) and centre-back → striker (10.4), forward. Engine
ones are mostly sideways or backward between defence and midfield: centre-back ↔ full-back 66 / 57, centre midfielder
↔ winger 63 / 56, and real passes to centre-backs are a median 17.2 m long (engine 32.2 m). The far target is no longer
freer than real; the AI still picks it.

The measured lines stay 2–7 m deeper than the sourced knots, and the defending blocks sit deep (mid-block line 30.4 m
against 37.5 m, low block 10.6 m against 17.8 m), which pins the attackers' offside line. Raising the in-possession
knots to [22, 47.5, 61.5] puts the lines on target (goals 5.55, changes of possession 305); raising the mid-block
line as well (`blk05_line_mid` −13 → line 36.6 m, −9 → 38.9 m) gives goals 5.5–7.3 and counter-attacks 41 per
match. Block heights have to be calibrated together with transition defending, so the sourced values stay for now.
The low block is also narrow (26.8 m against 35.7 m), most likely the BOX-04 narrowing.

ATT-10 needed a fix to pass its scenario test with the new shape: the collinear check applied to team-mates at any
distance, so most candidate points were "in line" with someone 20–30 m away, and the penalty separated nothing. It
now applies within 2 × `att10_crowding_radius`, like the same-line check, and its weight is a parameter
(`att10_collinear_penalty` 1.0, was a fixed 0.5).

### Long-ball choice (2026-10-28)

Engine 30–45 m passes were ≈ 600 per match (StatsBomb ×1.48: ≈ 110, 7.8 % of open-play passes). New benches:
- `long_choice`: every on-ball decision with all options (`Simulation::decision_log`), by pass band, and chosen long
  passes against the best short option.
- `long_defend`: what the defence does during an opponent's 25–45 m pass.
- `pressed_cal`: the ATT-25 receiver loss, expected against actual.
- `pass_cal`: the AI's pass p against what happened, by p and by the receiver's nearest opponent; lanes of completed
  forward passes (2026-10-29).
- `lane_cal`: what the lane defender of each forward ground pass ≥ 20 m does during the flight (2026-10-29).
- `passes`: now prints length × direction, and its `GEO` dump has the passer's y. The scratchpad scripts were
  `geo_switch.py` / `sb_switch.py` (switches of play), `sb_switch_fail.py`, `sb_dir.py` and `sb_long_receipt.py`.

What the data says:
- **The excess is switches of play.** Passes to the other half of the width crossing ≥ 25 m: engine 458 per match,
  real 98 (≥ 34 m: 256 against 34). 374 of the engine's 544 passes of 30–45 m were switches, against 47 of 110. Real
  ground switches are 97 % completed and high ones 58 % (12 % go out). Engine switches are completed 84–89 %.
- **The layer-1 utility prefers them.** With near-zero decision noise the engine plays as many (541 passes of
  30–45 m, 514 switches). For a chosen 30–45 m pass a short option was on offer 96 % of the time: long U +0.0073
  (p 0.95, receiver pressed 20 %) against the best short U +0.0033 (p 0.94, 15 m, pressed 36 %). The ATT-21 switch
  bonus applied to 70 % of the backward and 85 % of the lateral long passes.
- **The far targets are genuinely free.** At release the nearest defender is a median 17–19 m from a forward
  30–45 m target. Running straight there, the closest any defender could get by the arrival is 9–10 m. The actual
  distance at the first touch is about that (7.8–9.8 m). For real high balls it is 4.2 m at the receipt, measured on
  360 frames centred on the receiver.

Changes:
- **Ways to pass to one team-mate share one softmax weight** (`on_ball::softmax`): to feet, lofted and into his
  path. Before, a team-mate beyond 24 m had two options (ground and lofted), each with its own weight and noise draw,
  against one for a near team-mate. The group gets the weight of its best variant, shared in proportion to the
  variants' own weights. Shots already shared one weight. 30–45 m passes 600 → 544 per match.
- **PRS-02 reference during a pass**: the first defender and his cover orient to the reception point of the
  opponent's pass, not the ball in flight. The nearest defender at release now ends 12.0 m from the receiver
  (was 14.4 m).
- **ATT-21 only after pressure has been drawn** ("attack down one side to attract pressure", then switch): the
  carrier must be pressed (`att21_min_pressure` 0.5). The density ratio alone was nearly always met. The bonus is
  per team (`AttackingInstructions::switch_bonus`).
- **ATT-25 split by who is pressed**: receivers `att25_receiver_loss` 0.18, fitted with `pressed_cal` (the
  StatsBomb difference is 0.17). Carriers keep `att25_pressed_loss` 0.3; at 0.18 for both, tackles rose from ≈ 81 to
  104 per team. At 0.3 the AI overpriced pressed receivers by ≈ 1.6×.

Results (`passes` 10 matches, lab 40):

| | before | after | real |
|---|---|---|---|
| 30–45 m passes per match | 600 | 463 | 110 |
| switches ≥ 25 m / ≥ 34 m | 458 / 256 | 376 / 205 | 98 / 34 |
| 0–10 m / 10–20 m passes | 104 / 291 | 130 / 345 | 364 / 621 |
| goals (lab) | 3.83 | 3.62 | ≈ 4.2 |
| tackles per team | 81 / 80 | 91 / 86 | ≈ 25 |
| possession changes (`transitions` 40) | 270 | 283 | 162 |

Tried and dropped:
- A human field of view (`vision_half_angle_max` 150° → 100°): no change (459). Passers are already turned towards
  the target when they kick.
- Without the ATT-21 bonus at all there are still 347 switches.
- Removing it and lowering ATT-25 together gives 311 switches and 387 passes of 30–45 m.

**What is left (≈ 4× too many).** With equal geometry (block and in-possession widths within 2–5 m of WC2022), real
passers decline most free far targets that the engine takes. Two candidates, both outside the pass valuation:
- **One-step myopia.** A free carrier gains little by dribbling or holding (value of the end point only), so the
  long pass wins as soon as it is on. Real passers carry until pressed: nearest opponent at the pass 2.9 m, engine
  7.2 m. Keeping the ball also keeps the long pass available, and the one-step value cannot see that. A continuation
  value for keeping the ball (the best option still open from the end point) would address it.
- **Short support.** Team-mates within 15 m of the passer: engine 1.18, real 2.48. The nearest is a median 12.4 m
  away (real 8.5 m), and short options are pressed in a third of decisions.

### Continuation value and settle time (2026-10-28)

Superseded in part by set T (2026-11-02, see "Set T: tempo on the ball"): the continuation is now the carrier's
look-ahead level 1, the best pass he expects at his next decision with team-mates on their way to their offers and
opponents carrying on, instead of the best pass now faded with a half-life; and the settle time is split into getting
to the ball (unchanged by a scan) and looking up (shortened or lengthened by it).

The long-ball round found that engine passers release the ball sooner than real ones (`settle`, `support`, 10
matches; StatsBomb receipt → next pass or shot of the same player, 40 matches):

| | engine | real |
|---|---|---|
| first time (≤ 0.1 s) | 7 % | 18 % |
| 0.1–0.5 s after a controlled reception | 41 % | 7 % |
| median / > 1 s | 0.50 s / 33 % | 1.22 s / 59 % |
| nearest opponent to the passer | 5.2 m | 2.9 m |

Two causes, two changes:
- **Continuation value** (`on_ball`, value model): the utility was one step deep, so a carrier gained nothing by
  holding or carrying while a good pass was on. Keeping the ball now adds the best pass's lead over the value of the
  ball's location (`premium`, ≥ 0) to the end point's value, halved every `continuation_halflife_s` (1.0 s) of the time
  until the next decision: `onball_interval` when holding, the carry time when dribbling. Defenders close the lane
  meanwhile, so keeping a pass on is worth less than playing it (`tests/continuation.rs`).
- **Settle time** (`player.settle_decision_s` 0.35 s + `settle_composure_s` 0.25 s × (1 − composure)): after a
  controlling touch, the next decision, and so the next kick, came 0.1–0.35 s later. Real passes and shots after a
  controlled reception almost never come that soon: 8.5 % of the non-first-time passes within 0.5 s, 5 % of the
  shots. At 0.35 s the engine has 11 %; 0.4 s gives 2 %, 0.5 s none.

Results:
- Receipt → pass: median 0.50 → 0.78 s, > 1 s 33 → 40 %, 0.1–0.5 s 41 → 10 %. Nearest opponent at the pass 5.2 →
  4.5 m.
- Lab 40: goals 3.62 → 3.75, shots ≈ 38.8 → 33.1 per match, passes 652 / 731 → 602 / 653 per team, tackles
  unchanged (91 / 87). Possession changes 283 → 275 per match.
- 30–45 m passes 463 → 451 per match: still ≈ 36 % of passes.

What the measurements say:
- **The continuation value is a timing lever, not a pass-choice lever.** The best pass beats the value of the ball's
  location in only 35–39 % of free decisions (16–21 % pressed; `long_choice`), so the premium is usually 0. Where it is large, a good long
  ball is on, and keeping the ball only delays that pass.
- **The effect saturates.** Median receipt → pass at settle 0.5 s: 0.90 s without the continuation value, 0.94 s at a
  half-life of 1 s, 0.94 s at 2 s, 0.96 s at 4 s. Without fading at all (and the old settle time), dribbles were
  chosen in 72 % of pressed decisions: far too many.
- **The remaining timing gap is first-time play and the first decision after the settle.** At the first decision a
  free carrier passes 45 % of the time (pressed 37 %), so passes cluster at 0.5–0.9 s (39 %, real 12 %). Real players play 18 % of
  passes first time and otherwise usually take another touch or two.
- **The settle time costs goals in the box.** Settle 0.5 s cut open-play shots from 743 to 621 and goals from 55 to
  42 in 20 matches (0–11 m: 91 → 67 shots). The quick controlled shot had been standing in for first-time finishing:
  no engine shot inside 6 m is struck first time, while 47 % of real shots after a reception are. Those finishes come
  mostly from crosses and cut-backs (M4). 0.35 s keeps the goal rate.
- **Long balls are long because the engine makes them safe.** The AI expects a chosen 30–45 m pass to succeed 95 % of
  the time, about the same as its best short option. In the engine they are completed 89 % of the time; real 30–45 m
  forward passes 55 %, high ones 37–58 %. Two causes:
  - Real high balls are contested: the nearest defender is 4.2 m away at the receipt, and 29 % of receipts have one
    within 2 m. In the engine it is 7.8 m, and only 3 % of lofted 30–45 m balls lead to an aerial duel.
  - Lofted execution: 1–2 % of engine long balls go out of play, against 8–12 % of real ones.

## Contested high balls and lofted accuracy (2026-10-28)

The question: real high balls of 30–45 m are completed about half the time, engine lofted ones 89 %. Was that the
contest in the air or the accuracy of the kick? Measured on 166 StatsBomb matches with 360 frames (`high_balls`
example; scratchpad `sb_high_balls.py`, `sb_aerial2.py`).

**Completion depends on the race at release, and the engine already got that right.** The race margin is the
nearest opponent's distance to the pass's end point minus the nearest team-mate's, at the moment of the kick:

| margin | < −5 m | −5–0 m | 0–3 m | 3–6 m | 6–10 m | ≥ 10 m |
|---|---|---|---|---|---|---|
| real completed | 6 % | 33–34 % | 55–56 % | 75–76 % | 83–85 % | 97–98 % |
| engine before | 8–11 % | 39–41 % | 54–61 % | 71–78 % | 85–89 % | 95–96 % |
| engine now | 15–20 % | 46–47 % | 66–67 % | 71–76 % | 81–87 % | 94–95 % |

(Ranges: all / forward passes; "before" from the runs before this round, "now" at the final settings, 20 matches.)

The difference is the mix. Real high balls are contested (margin < 0) 33–36 % of the time and free by 10 m or more
23–27 %. The engine's are contested 14–17 % and free 47–57 % of the time, and the engine plays ≈ 230 lofted
30–45 m passes per match against 58 real; ≈ 130 of them go to a team-mate with no opponent near (real 16). The AI
prices them about right: its p is 82 % for a realised 81 %. By margin band it is within a few points except the
contested −5–0 m band, where it now expects 58–62 % and gets 46–47 % (the < −5 m band is measured where the ball was
won, so its p is not comparable). So the excess of long balls is a choice and situation problem, not a pricing or
execution problem.

**Lofted accuracy is unchanged.** The earlier estimate (lofted error ×1.5) compared all real passes with engine passes
to feet; real high balls into space for a runner inflate the real end-point error. Compared by margin, completion
matches, so the execution error stays. `execution.lofted_factor` (1.0, unused by any data file) was removed.

**What did not match was the duel at the end.** Before this round the defender at a completed receipt stood a median
2.8 m away when he had been 0–5 m from the end point at release (real 1.0 m, 64 % within 2 m) and 3.7–3.9 m from
5–10 m (real 2.1 m). Aerial duels ended 2–3 % of these passes (real 14–15 %), and opponents headed 12–15 % away
unchallenged (real ≈ 7 %). Three changes:
- **IND-07 attack the high ball.** The defender who can reach the receiver's reception point no later than
  `ind07_challenge_slack_s` (1.0 s) after him goes straight to it instead of stopping at a contain distance (PRS-02
  only took over when he was first). Per team `DefendingInstructions::challenge_slack` (−1 = only when first).
  Sweep over the slack, forward passes (real in the last column):

  | slack | −1 s | 0.4 s | 1.0 s | 2.0 s | real |
  |---|---|---|---|---|---|
  | contact median, 0–5 m band | 2.8 m | 1.0 m | 1.3 m | 1.8 m | 1.0 m |
  | contact median, 5–10 m band | 3.7 m | 2.8 m | 2.5 m | 2.1 m | 2.1 m |
  | completed, 5–10 m band | 70 % | 62 % | 58 % | 64 % | 37 % |
  | aerial duels won by the passing team | 54 % | 50 % | 48 % | 54 % | 41 % |

  1.0 s fits best overall. With ≈ 12–21 such passes per band and match, 20 matches cannot separate 0.4–2.0 s
  much.
- **ATT-24 contested receiver attacks the ball.** A receiver under pressure meets a high ball up to his header height
  instead of control height ("just before your opponent" [USS-D p.73]). Otherwise the defender headed it while the
  receiver waited for it to drop.
- **`aerial_contest_radius` 1.2 → 1.6 m**, so a defender at the receiver's shoulder makes it a duel (11–14 %). 2.0 m
  gives 13–17 %, but unchallenged opponent headers fall to 5 % (real ≈ 7 %).

A bug turned up: `Simulation::reception` planned with `self.traj`, whose times are relative to when it was predicted
(up to 0.2 s ago). Receivers planned to take the ball up to 0.2 s late, and the "free receiver waits for it to drop"
check could skip past head height. Reception now predicts the flight from the ball as it is; lofted balls out of play
off a receiver's head fell from ≈ 30 to ≈ 1 per match.

Results (20 matches):
- Contact medians at completed receipts: 0–5 m band 1.3–1.8 m (real 1.0–1.1), 5–10 m 2.5–2.7 m (2.1–2.2),
  10–15 m 4.5–4.7 m (5.1).
- An aerial duel ends 11–14 % of lofted 30–45 m passes (real 14–15 %); the passing team wins 48–50 % of them (real
  41 %). Opponent headers with no duel 6 % (real ≈ 7 %).
- Lofted 30–45 m passes 252 → 232 per match, 81 % reach a team-mate first (forward 79 %; real high 49 % / 45 %).
- All 30–45 m passes 451 → 402 per match. The `passes` example counts a pass complete only when the team then
  controls the ball, so a receiver's header that the opponents pick up fails there ("own header", 8–17 % of 30–45 m
  passes by third). Completion: forward 89 → 73 %, backward 94 → 86 %.
- Lab 40: goals 3.83 (3.75), passes 589 / 601 per team at 87–88 %, possession changes 271 (275).

Still open:
- Receivers win contested races a little too often: −5–0 m 46–47 % vs 33 %, 0–3 m 66–67 % vs 55 %. The passing
  team's aerial win share is 48–50 % vs 41 %.
- `duel::header` direction is crude: a knock-on (x + 15 m, y × 0.5), or (x + 30 m, ±20 m) in the own half.
- Second-ball positioning: [FFA-NC] p.252 names it ("Positioning/timing (2nd ball)") but no federation source found
  says where to stand. Deferred.
- The long-ball volume: the next levers are the choice of long balls to free team-mates and lofted passes into the
  path of runners (real nearest team-mate at release 7.7 m from the end point, engine 4.8–5.0 m).

## Reading the pass, and why long balls still win (2026-10-29)

Continuing the long-ball choice. The question this round: are long ground passes too easy because defenders near the
line cannot get to them, and does the AI misjudge which passes are safe?

**Pricing.** Two fixes to how the AI judges a pass:
- `evaluate_pass` now prices every pass to a team-mate with the first-arrival race along the path that lofted passes
  already used (`analysis::race`). The old ground-pass check (opponents until the arrival, the receiver standing at
  the target) predicted 53 % for passes to marked receivers that were kept 80 % of the time. Open-play passes rose
  from ≈ 1170 to ≈ 1490 per match and lab completion fell from 87–88 % to 77–78 % (real ≈ 82 %): the AI now plays
  into marked team-mates.
- `reach::reception_point` falls back to the first reachable point after the closest one, so a free receiver drops
  back to control an overhit lofted ball instead of heading it. Lofted passes at p ≥ 0.95 kept 84 → 92 %.

**The lane.** Measured on StatsBomb 360 (60 matches, scratchpad `sb_lanes.py`): the nearest outfield opponent to
the middle of a completed forward pass's line (10–85 % of the way) at release.

| completed forward ground passes, lane < 2 m | 10–20 m | 20–30 m | 30–45 m |
|---|---|---|---|
| real | 18 % | 11 % | 7 % |
| engine, no cue | 22 % | 25 % | 23 % |
| engine, `pass_cue_s` 0.2 s | 15 % | 15 % | 14 % |

The new `lane_cal` example follows the lane defender of each forward ground pass ≥ 20 m. The excess sat near the
passer: of completed 20–45 m passes, a defender < 2 m from the line at 0.10–0.33 / 0.33–0.60 / 0.60–0.86 of the way
let through 11.9 / 5.0 / 4.7 % (real 6.0 / 3.6 / 0.7 %). Reacting only to the strike, a defender 10 m from the
passer is too late for a ball that passes him 0.6 s later. Real defenders read the pass before it is struck [FA-EFL
intercepting sessions], so **IND-08** starts every player's reaction to a pass (not a shot) `pass_cue_s` before the
strike, never earlier than the release itself (`reach::pass_reaction`). The team brain's chaser, the AI's own
interception odds and the next decisions all use it. Sweep 0 / 0.1 / 0.2 / 0.3 s (10 matches each): 0.2 s gives the
lane shares above and 8.9 / 2.3 / 2.6 % along the line. 30–45 m passes went 421 → 408 per match, all open-play passes
1562 → 1330.

How readily a defender touches a pass going by him did not matter. That chance is now `duel::pass_touch_prob`
(`player.pass_touch_base` 1.25 − `pass_touch_per_mps` 0.04 × ball speed, clamped 0.35–0.95, scaled by
anticipation; the same numbers as before, now in the config), and a sweep from 0.9 flat to the current curve moved
the lane shares by ≤ 2 points.

**The possession value curve, fitted (the default since set C).** From 166 StatsBomb matches (scratchpad
`sb_receipt_value.py`, `fit_value.py`), the xG a team still gets in a possession after a free pass receipt rises
from 0.0097 (0–25 m from the own goal line) to 0.0136 (40–52.5 m), 0.024 (75–85 m) and 0.0735 (95 m on). A fit
`base + progress · s^power · exp(−(y/width)²) + share · xg` gives base 0.0083, progress 0.028, power 1.84, width
39 m, shot share 0.31 (loss 0.015 against 0.18 for the current curve). It is much flatter in the own half than the
current curve (0.004, 0.06, 0.55). With it (10 matches), lanes came out right (< 2 m 10 / 13 / 10 %), completion 85 % and
tackles ≈ 79 instead of ≈ 105 per team. But 30–45 m passes rose to 453 per match, the backward ones from 113 to 175: full-backs
and wingers under pressure (nearest opponent median 2.7–3.9 m) played 30–45 m back to centre-backs and the keeper.
With a flat own half, going back costs little, and the short options are worse than they should be (below). The
fit was kept out until short passes were as safe as real ones; set C of the watch round (2026-11-02, below) adopted
it with the turnover cost and the in-behind premium that go with it.

**Why long balls still win.** The `long_choice` bench at the final settings: of 368 chosen 30–45 m passes per match,
a short pass (< 20 m) was on offer 96 % of the time. The long forward ones gain a median +0.53 of the value scale at
p 0.82 (U +0.0084); the best short option gains +0.02 at p 0.91 with its receiver pressed half the time (U +0.0041).
Zeroing the ATT-03 and ATT-17 bonuses left 409 per match. With the fitted curve, no ATT-01/03/17 bonuses and
near-zero decision noise there were still 424, the layer-1 best chosen 80 % of the time. So the utility itself
prefers them, because the engine's odds are inverted against real ones:

| completion (engine / real) | forward | lateral | backward |
|---|---|---|---|
| 10–20 m | 79 / 83 % | 80 / 92 % | 91 / 97 % |
| 20–30 m | 75 / 74 % | 75 / 91 % | 89 / 97 % |
| 30–45 m | 72 / 55 % | 70 / 90 % | 88 / 96 % |

Short lateral and backward passes are lost two to three times as often as real ones, long forward ones succeed
far more often. With the same number of free team-mates within 15 m, real passers pick a 30–45 m pass 5–11 % of the
time, the engine 18–38 % (`support`, StatsBomb `sb_support.py`). The support count itself is now close to real
(team-mates within 10 / 15 m of the passer 1.02 / 2.15, real 1.09 / 2.48).

Results (lab, 40 matches): goals 3.30, shots 27.2 per match, passes 648 / 701 per team at 79–80 %, tackles 106 /
100 per team, interceptions 127 / 120. All tests and clippy pass.

Next levers, by this evidence:
- Why short lateral and backward passes are lost (12 and 6 points too often), and to whom: lanes, the receiver's
  first touch under pressure, or defenders stepping in front of marked receivers.
- Why long forward targets are free enough to be completed 72 % (real 55 %): the far side of the block and the
  last line's marking.
- Then the fitted value curve.

## Attribute scale and the perfect player (2026-10-29)

The user's check of the three-layer idea (a perfect player follows the tactical optimum and makes the optimal
decision; real players deviate by tactical, personal and technical attributes) with the new `layers` example
(every attribute of every player set to one value, `ATTRS= EGO= FLAIR= AWAY= CONF= ENERGY= TEMP=`). The structure
held, but a 20/20 player picked the layer-1 best only 60 % of his on-ball decisions (`temperature_best` 0.06 was a
floor), and tactical reliability was a weighted sum, so a 20/20 player at half confidence still drifted little.

Changes:
- **Attributes are 0–100** (`norm` = v / 100). The squads and `Attributes::default()` were mapped exactly from the old
  1–20 values ((v − 1) / 19 × 100), so their effective values are unchanged.
- **Tactical reliability is multiplicative**: `reliability = norm(tactical_awareness) × f(confidence) × f(energy) ×
  f(attunement)`, with `f(x) = floor + (1 − floor) · x` and `ai.reliability_confidence_floor` 0.6, `_energy_floor` 0.6,
  `_attunement_floor` 0.7. Drift σ = `drift_sigma_max` (2.0 m, was 3.0) × (1 − r), score noise σ =
  `position_noise_max` (0.4, was 0.6) × (1 − r), and the off-ball re-evaluation interval runs from
  `offball_interval_max` to `_min` with r (it used concentration and anticipation). The maxima were lowered so that
  the built-in squads (tactical awareness ≈ 60, confidence 0.5, attunement 0.85: r ≈ 0.48) keep the drift ≈ 1.0 m
  they were calibrated with; `tests/movement.rs` guards it.
- **On-ball temperature** = `temperature_worst` + (`temperature_best` − worst) × norm(decisions) × f(confidence) ×
  f(energy), with `temperature_best` now 0: a 100-rated decision-maker at full confidence and energy takes the
  layer-1 (+ layer-2 + personality) argmax. Composure only enters through pressure.

`layers`, 4 matches each, egotism 0 and flair 50 so the personality layer is neutral:

| everyone | drift from the target | on-ball choice = layer-1 best | within 0.0005 of it |
|---|---|---|---|
| 100 at confidence 1, energy 1 | 0.00 m | 96 % | 98 % |
| 100 at confidence 0.5 | 0.50 m | 64 % | 74 % |
| 100 at energy 0.5 | 0.50 m | 65 % | 75 % |
| 60 | 1.00 m | 45 % | 53 % |
| 20 | 2.01 m | 26 % | 31 % |

The 4 % the perfect player does not pick are ties (equal utilities split by the held Gumbel draw) and options that
change between re-evaluations. Energy is still a constant 1.0 in matches (fatigue drain is M6).

Found by the same bench and fixed in the next section: the attribute → outcome slope was far too steep (uniform
squads 68 v 47 gave 11.6–0.25 goals and 54 v 1.8 shots).

## Attribute slope (2026-10-29)

How much better a squad plays for being N points better in every attribute. Measured with `layers` (`AWAY=` gives
the away side its own value; `GROUP=` restricts the gap to one attribute group or a comma list; `SIDES=1` prints
per-side passes, tackles, interceptions, miscontrols, possession by third, turnover third and pass direction). The
two built-in squads are alike, so no earlier calibration saw this.

**Where the slope came from.** No single mechanism: at a 68 v 47 gap each attribute group alone gave 2–4× the
shots (physical 3.0, technical 3.8, mental 1.9) and all together 30× (54 v 1.8), because every contest in a
possession chain (pass error, interception race, receiver's touch, tackle, aerial, decision noise) leaned the same
way and the chances multiply. The weak side did not play safe (forward share 60 vs 64 %, chosen p 0.83 both): it
completed 85 % of 876 passes and still spent 1053 s in its own third and 105 s in the final third, losing the ball
in the middle third 183 times. Two cliffs made it worse: a defender only tackled at a win probability ≥ 0.65, which
a 47 against a 68 never reached (32 tackle attempts per match), so the carrier ran straight past him; and the goals
per xG of the strong side ran 1.5–1.7 (1.2 at equal strength).

**Changes**, each recentred on the calibrated 58-rated player so the built-in squads keep their odds:
- IND-03 tackle gate: a carrier about to get goal-side of an uncovered defender within `ind03_last_ditch_s` (0.3 s)
  is challenged whatever the odds (DEF-02 cover takes him otherwise), and a loose touch clearly nearer the defender
  (`duel.tackle_loose_margin` 0.3 m, not from behind) is always contested; the ≥ 0.65 gate now only covers a ball
  the attacker has. Challenges per team rose from ≈ 100 to ≈ 170 in the lab (they count every poke at a loose
  touch; real duels per team ≈ 90–110 → ≈ 130–160 scaled).
- Duel slopes halved: `tackle_slope` and `aerial_slope` 5 → 2.5. The one-sided duels separate the slope in the
  situation's difficulty (unchanged, 5 and 4) from the slope in the player's skill around `duel.skill_pivot` 0.58:
  `first_touch_skill_slope` 2.5, `save_skill_slope` 2.0.
- Execution error multiplier `execution.error_scale_worst` / `_best` 1.32 / 0.60 (was a fixed 1.6 − 1.2·skill,
  i.e. 1.6 / 0.4); 0.90 at 0.58 either way. The dribble touch error uses the same.
- Kinematic ranges narrowed ≈ 20 % around the same average: top speed 7.4–9.8 m/s (was 7.0–10.0), acceleration
  6.6–10.0 (6.0–10.5), deceleration 5.4–7.7, lateral 4.5–7.1.
- Decision temperature concave: `temperature_worst` 0.23 (0.115 since set C, 2026-11-02) with `temperature_exp` 2 (temperature = worst · (1 −
  skill²)); the 58 keeps the temperature he had with 0.35 linear, a 47 gets 0.17 instead of 0.19, and a 100 stays
  at 0.

The remaining slope by group (58 v 47, 24 matches, shots ratio): decisions 1.26, composure 1.25, pace/acceleration
1.40, agility/balance/strength/jumping 1.49, first touch/technique 1.35, dribbling/tackling/marking 1.26,
anticipation/concentration 1.16, tactical awareness 1.15, passing/crossing/finishing 1.18 (measured before the
physical narrowing and the concave temperature). They still multiply.

| squads (every attribute) | before: goals, shots | after: goals, shots |
|---|---|---|
| 68 v 68 | 1.4–1.1, 13.5 v 13.0 | 1.4–1.9, 11.8 v 11.9 |
| 58 v 47 | 3.9–0.4, 18.0 v 4.5 | 2.2–0.8, 14.4 v 5.6 |
| 68 v 58 | 4.0–0.9, 23.0 v 5.6 | 2.5–1.0, 17.5 v 7.1 |
| 68 v 47 | 11.6–0.25, 54 v 1.8 | 4.4–0.5, 24.5 v 3.1 |

So an 11-point gap in everything is now ≈ 2.5× the shots and ≈ 2.5–3× the goals, like a side a league tier better,
and a 21-point gap is a cup thrashing (8× the shots). The weak side at 21 points still gets only ≈ 3 shots where a
real non-league side gets ≈ 5, mostly from set pieces and direct play the engine does not have (v1 is open play;
the weak side keeps circulating at 84 % completion instead of going long). The lab (40 matches): goals 2.53 (3.30
before; 40-match noise ≈ 0.5), shots 12.2 / 10.4, completion 80 / 81 %, challenges 169 / 177; off-ball mean speed
3.3–3.5 m/s and walking 30–34 % (`tests/movement.rs` guards widened from 3.4 / 30 % to 3.6 / 28 %).

## Direct play and the second ball (2026-10-29)

Asked for: the weak side's direct play (the attribute-slope round left a 47-rated side with ≈ 3 shots against a 68).
Measured with `layers` (`SIDES=1` now also prints passes by length, forward passes ≥ 30 m and what follows them,
possessions won in the own third that reach the final third within 10 s; `STRIKER=pace=72,heading=72` gives the away
strikers their own attributes; `DUMP=n` prints the weak side's own-third decisions with every option) and `pass_cal`
(now also headed passes, and aerial outcomes by who won the duel).

**What was wrong was the ball in the air, not the choice.** A weak-side defender's long ball to his marked striker
was priced at 0.05–0.35 because `analysis::race` counted a defender who reached the ball a fraction earlier as a
loss, while the sim settles that as an aerial duel by heading skill; lofted passes priced below 0.5 were completed
52 %. And the sim's headers were blind: every header was knocked 15–30 m forward, so a defender who won the ball in
the air gave it back. In StatsBomb (40 matches, open-play high passes of 30–45 m: 48 % complete) the kicking team is
next in control after an incomplete one 11 % of the time, after a lost aerial duel 24 %; the engine gave it back 67–83
% after a lost duel and ≈ 65 % after an opponent's free header. That made hoofing pay in the engine for the wrong
reason, and the on-ball AI did not know it.

Changes:
- **Headed passes.** A header not at goal is a headed pass to the best-placed team-mate within the header's range
  (≥ 4 m of space, the freer and the nearer to 15 m the better, forward preferred), a clearance long and wide only
  with nobody to head to; a 35° lob at up to 12–20 m/s by skill. It is a real pass: `EventKind::Pass { headed: true
  }`, its receiver prepares for it and the opponents read it (IND-07 entry in the catalog). The benches that judge
  the on-ball AI's passes skip headed ones. After a lost aerial the kicking side is now next in control 0–3 % of the
  time (real 24 %; the opponent's headed pass counts as his control, as a StatsBomb headed pass does).
- **Aerial pricing.** In `race`, above control height a header the receiver contests (first to it, or within
  `aerial_contest_radius` of the opponent who is, both with the slack the ball's speed gives) is an aerial duel by
  `aerial_win_prob`, then a loose knock-down kept `ai.aerial_won_keep` (0.9) / `ai.aerial_lost_keep` (0.05) of the
  time; an opponent's free header is loose too (`aerial_lost_keep`), a keeper's catch is not. A free receiver lets
  the ball drop (as `Simulation::reception` does), so only the contested share of his arrival is taken at head
  height. `PassEval::p_air` carries the knock-down share and the on-ball AI values it at `ai.aerial_value_factor`
  (0.5) of the ball at the target. Calibrated with `pass_cal`: lofted passes predicted vs completed 41/36, 59/53,
  75/76, 86/82, 93/91, 97/96 % by band (before: 31/52 in the lowest band, 85/72 and 93/83 higher up).

Effects (`high_balls`, `passes`, 12 matches; lab 40): lofted 30–45 m passes 164 per match (≈ 200–230 before; real
56), contested (margin < 3 m) 35 % (real 33–36 %), ending in an aerial duel 20 % (11–14 % before, real 14–15 %), an
opponent's free header 10 % (real ≈ 7 %), completed 75 % (real 48 %: the engine's free targets are still too many).
Lab goals 2.65 (2.53), shots 13.9 / 12.1, passes 748 / 826 at 78–79 % (headed passes now count: ≈ 45 per team),
interceptions 159 / 149, mid-block length 29.2 m (36.0; real 27.2), counter-attacks 25.7 per match (39.1).

| squads | before: goals, shots | after |
|---|---|---|
| 68 v 68 | 1.4–1.9, 11.8 v 11.9 | 1.6–1.7, 15.0 v 14.2 |
| 58 v 47 | 2.2–0.8, 14.4 v 5.6 | 2.4–0.7, 16.6 v 4.9 |
| 68 v 47 | 4.4–0.5, 24.5 v 3.1 | 5.3–0.4, 33.8 v 3.1 |
| 68 v 47, away strikers 72 in pace / acceleration / jumping / heading / strength | – | 5.6–0.1, 30.1 v 3.6 |

**So the weak side's shots did not move**, and the top gap got steeper (the honest second ball punishes the side
that loses the ball). With a target man the weak side plays 50 % more long balls into the final third (31 vs 20) and
completes them no better (56 %). Against a side that is 21 points better in everything, open-play direct play does
not make shots here, and that is about what the data says: a non-league side against a top-flight one gets ≈ 5
shots of which ≈ 2 come from set pieces, which v1 does not have (M7). Two things remain open:
- The one-step valuation: a weak side pressed in its own third keeps choosing the 0.9 short pass to a team-mate who
  is pressed in turn (it plays ≈ 1050 passes at 83 % and loses the ball in the middle third ≈ 175 times), because
  each step is safe on its own; the cost of the chain is not priced. A long ball's p (≈ 0.45) and the knock-down
  value (0.5) lose that comparison every time.
- Free lofted targets are still too many (completion 75 vs 48 %, volume 3×): the long-ball choice problem noted
  under "Reading the pass" stands.

## One-step valuation (2026-10-29)

Asked for: the chain the direct-play round left open, a weak side pressed in its own half choosing the safe short
pass to a team-mate who is pressed in turn, each step safe on its own. Measured with `pressed_cal` (now per side with
`ATTRS=68 AWAY=47`, with the cause of each loss, the time on the ball, the team's loss within 6 s and the receiver's
free outlets) and the new `hold_cal` example (every hold decision by the pressure expected at the next decision,
against whether the carrier kept the ball to his next decision and to his next kick, and how his best pass then
compared with now; `DUMP=n` prints pressed holds with every option).

What the chain is, and is not:
- Not the options: a pressed receiver loses the ball before his next pass or shot 27–35 % of the time whether he has
  1 or 3 free team-mates within 30 m; a free one 3–7 % (StatsBomb 21.7 % under pressure, 4.9 % free).
- Not the next pass: a team loses the ball within 6 s after a pressed reception 55–66 % of the time, after a free one
  34–38 %, and the difference is the receiver's own loss. The one-step valuation prices it at 0.18 × pressed.
- Not the side: the 47-rated side's receivers lose no more than the 68 side's (23–28 % against 29–35 % pressed).
- Every loss is a tackle after a median 1–2 s on the ball: pressed receivers hold. The hold is cheap per decision
  (keep given 0.68–0.75, actual 0.97 at full pressure, tackled 3–5 %) and dear per spell (18–30 % lost before the
  next kick), and at every decision the best pass is no worse than now, so holding is right per step and wrong over
  the spell. That is the one-step myopia, and it sits in the carrier, not the receiver.

Changes:
- **ATT-06 in the valuation.** The continuation a carrier is credited for keeping the ball (the best pass on offer,
  kept for the next decision) is scaled by (1 − pressed at the next decision) for a hold and by (1 − pressed at the
  end) for a dribble: under pressure the ball is played now. In the own half the credit was ≈ 0.0002 on values of
  ≈ 0.01, so the pressed receivers' losses did not move (28–32 %); in the final third it is larger and the change
  releases the ball earlier. Lab 40: goals 3.33 (2.65), shots 15.6 / 13.1 (13.9 / 12.1), completion 77–78 %,
  tackles 170 / 181, mid-block length 30.8 m; 68 v 47 → 5.1–0.4, 35 v 3 shots; 58 v 47 → 3.4–0.7, 18 v 5; 68 v 68 →
  1.5–1.7, 18 v 12.
- **Tried and reverted: pricing the receiver's loss at what the engine does** (`att25_receiver_base_loss` 0.05 +
  `att25_receiver_loss` 0.25 × pressed). Honest per step, wrong as a fixed point: passes to pressed team-mates lost
  value against carrying and holding, whose own chains are not priced either, so carriers passed less (651 / 727 passes
  at 75–76 %, from 760 / 844 at 77–78 %) and carried more, tackles rose to 208 / 220 per team, receivers lost more
  still (37–40 % pressed, 8–10 % free), 68 v 47 went to 7.1–0.3 with 50 shots. The parameter stays at 0 with the
  finding on it. The engine's equilibrium is that of the pair (receiver 0.18, carrier 0.3), not of each measured alone.

The weak side: nothing here changes its shots (3 at 21 points, 5 at 11). Its pressed carrier in the own half has
passes at p 0.2–0.6, dribbles at 0.4–0.7 and a hold at 0.6–0.7, all priced about right per step, and no clearance
outside the own third; the least bad is to keep the ball and be tackled. What a real weak side does there is get rid
of it: the long ball is priced at 0.26–0.30 (its aerial duel is honestly lost against 68-rated centre-backs) and its
second ball is now 0–3 % against StatsBomb's 24 %, because the engine's headers are too accurate (70 % of headed
passes complete). That, not the valuation, is the next lever: headed-pass execution error by heading skill and
the duel, which would raise `aerial_lost_keep` towards 0.2 and the long ball's price with it.

## Headed-pass error and the loose header (2026-10-29)

Asked for: the headed-pass execution error, the lever the one-step-valuation round named for the second ball.
Measured first (StatsBomb open play, 40 matches, scratchpad scripts `sb_headers.py` and `sb_aerial_out.py`):
headed passes complete 61 % (0–10 m 67 %, 10–20 m 59 %, 20–35 m 51 %), and the engine's already did (58 / 61 / 53 %
by the same bands, `pass_cal`), so the error itself was not the gap. The gap was what a won duel becomes: in
StatsBomb the winner's first action is a pass 53 %, a headed clearance 29 %, a shot 10 % of the time, and a headed
clearance is next controlled by the clearing side only 20 % of the time (the other side 66 %); in the engine every
won header with a team-mate in range was an aimed pass, so the kicking side was next in control after a lost aerial
0–3 % of the time (StatsBomb 24 %).

Changes (`Simulation::header`, now told whether the header was contested):
- A header won in a duel is headed away loose, long and wide, with probability `duel.contested_header_loose` ×
  (1 − heading) (0.9: 29 % for a 68-rated header, 48 % for a 47) instead of to a team-mate; a contested header's
  direction error is 1.5× a free one's.
- The AI's knock-down keeps re-measured: `ai.aerial_won_keep` 0.9 → 0.6, `ai.aerial_lost_keep` 0.05 → 0.28.

Effects (`pass_cal` 8, `high_balls` 8, lab 40, `layers` 16): after a lost aerial the kicking side is next in
control 20–25 % of the time (real 24 %), after a won one it keeps 56–71 %; lofted passes predicted vs completed
42/37, 60/62, 75/69, 86/87, 93/93, 97/96 %; headed passes 147 per match (260: the loose header ends the re-heading
chain; real ≈ 50 at the engine's time in play), 56 / 67 / 73 % complete by band; lofted 30–45 m 149 per match
(real 56), 74 % complete, aerial duel 16 % (real 14–15), opponent's free header 11 % (real 7). Lab goals 2.85
(3.33), shots 13.6 / 12.4, completion 79–80 %, tackles 173 / 178, mid-block length 29.3 m. Gap table: 68 v 47 →
6.6–0.2, 35 v 3.1 shots; 58 v 47 → 2.5–0.4, 17 v 3.6; 68 v 68 → 1.5–0.9, 17 v 12. The weak side's long balls into
the final third rose from 21 to 26 per match at 57 %, its shots did not: the second ball now falls to it as often
as in the data, and it still cannot turn the middle third into shots against a side 21 points better. Open as
before: lofted volume 2.7× real, headed volume 3×, set pieces (M7).

## Lofted volume, hang time and two stuck states (2026-10-29)

Asked for: the lofted volume (lofted 30–45 m passes 149 per match after the headed-pass round, StatsBomb ≈ 56).

What a pass is worth afterwards, measured first (new bench `pass_payoff`, scratchpad twin `sb_pass_payoff.py`: the
xG its team still gets in the same possession, by origin band and length × direction). In StatsBomb, 30–45 m
forward passes from the own 25 m complete 55 % and are worth less than short forward ones (0.0063 vs 0.0109 xG
after); from 25–65 m they complete 60–63 %. The engine's completed 73–86 % and paid as much as or more than short
forward passes: in the engine long balls genuinely paid.

Hang time. StatsBomb open-play high passes of 30–45 m reach the next event (the receipt or the contest) after a
median 2.45 s when complete and 2.56 s when not (`duration`, `sb_flight_time.py`). The engine's took 2.14 s from
kick to first contact at the fixed 0.5 rad launch angle in `on_ball::passes`. It is now `ai.lofted_launch_angle`,
calibrated on that hang time (`passes` prints "kick to first contact"; `high_balls` 10, lab 40):

| Launch angle | Kick to first contact | Lofted 30–45 m per match | Complete | Aerial duel at the end | Lab goals |
|---|---|---|---|---|---|
| 0.5 rad | 2.14 s | 150 | 74 % | 17 % | 3.10 |
| 0.6 rad | 2.40 s | 114 | 70 % | 25 % | 3.08 |
| **0.65 rad** | **2.52 s** | **100** | **68 %** | **28 %** | **3.05** |
| 0.7 rad | 2.62 s | 88 | 63 % | 36 % | 2.88 |
| StatsBomb | 2.45–2.56 s | ≈ 56 | 48 % | 14–15 % | |

A longer flight lets defenders reach the drop, so fewer long balls are worth it; the AI's lofted p stays
calibrated (predicted / completed 43/36, 60/69, 76/78, 85/86, 93/94, 97/96 %). The aerial-duel share overshoots
(28 % vs 14–15 %). Shrinking `duel.aerial_contest_radius` 1.6 → 1.3 / 1.1 / 0.9 m left it at 25 / 24 / 25 %: the
challengers stand at the drop, not near it, so the radius stays.

Where the rest of the excess is (`high_balls` now splits free and contested passes by role and by the aim's
distance from the touchline; StatsBomb 360 subset `real_high_balls166e.txt`, scripts `sb_long_who.py`,
`sb_switch_who.py`, `sb_switch_line.py`):
- Free (no opponent within 15 m of the contact point at release): 49 per match, 97 % complete, 86 % lateral
  ≥ 25 m, to FB 51 % / W 24 % / CB 24 %, played most often by a striker dropped into the middle third (20 %) or the
  keeper (17 %). Contested: 54 per match, 42 % complete, to ST 56 %. In the 360 data 45 % of high 30–45 m passes
  are free in the same sense and complete 92 % (engine 48 %, 97 %).
- By margin (nearest opponent − nearest team-mate at release) engine completion is at or below real in every band
  (−5–0 m 33 vs 34 %, 0–3 m 48 vs 55, 3–6 m 62 vs 76, 6–10 m 77 vs 85, ≥ 10 m 97 vs 98), but 45 % of its long
  balls have a margin ≥ 10 m (real 27 %).
- Real own- and middle-third high switches (≥ 25 m across): 13.5 per match, 71 % complete, 5 % out, received a
  median 6.7 m from the touchline. Real strikers play 4 % of the middle third's long high passes.

So the lofted pass itself now resolves as in the data; what is left is how often it is chosen (≈ 1.8× real, free
and contested alike) and how often a far target is completely free. The larger excess is still on the ground:
30–45 m passes 379 per match (real ≈ 110), ≈ 275 of them ground passes (real ≈ 46), while passes under 20 m are 528
(real ≈ 990). The cause found in "Reading the pass" stands: short passes are lost about twice as often as real
(middle third 10–20 m 81 % complete vs 92 %), so long passes win the comparison.

Two stuck states, found through seeds with absurd numbers:
1. A carrier took the ball over his own goal line unseen. A centre-back running onto a parried ball collected it
   0.1 m inside the line; `attach_ball` moved it to his foot, already beyond the line, before `step_ball`, and
   `check_lines` only sees crossings within the physics sub-steps. He then held it 5 m behind the line for ≈ 35
   minutes of live play (seed 10: 7212 holds). `Simulation::step` now remembers the ball before `attach_ball` and,
   if the ball is not held and no goal or out was found, checks that whole move with `check_lines` (now public).
   Test `tests/lines.rs`.
2. A ball came to rest on top of the crossbar. `swept_hit` returned a hit at t = 0 whenever the ball started in
   contact, and `reflect` put it back every step, so it stayed there for ≈ 15 minutes while players headed it
   (seed 14: 40 shots, 6.6 xG). A ball in contact now only hits if it moves into the bar. Unit test
   `a_ball_resting_on_the_crossbar_can_be_headed_off_it`.
- Guard: `Calibration::longest_still_s`, the longest live spell with the ball within 1 m of one spot. The lab prints
  it (target < 30 s) and flags any seed over 30 s; `full_match_is_sane` asserts it. With both fixes: 7.4–10.8 s in
  every 40-match lab, 7.8 s over 80 matches. Earlier labs including seeds 10 and 14 carried these states.

Effects (lab 40): goals 3.05 (2.85), shots 15.1 / 12.1, completion 78–79 %, tackles 171 / 181, mid-block length
29.1 m. Gap table (`layers` 16): 68 v 47 → 5.75–0.25, 32.3 v 2.9 shots; 58 v 47 → 2.56–0.56, 15.4 v 5.6; 68 v 68
→ 1.44–0.94, 14.0 v 14.2. Bench changes: `passes` prints kick to first contact; `long_choice` prints holds by role
and dumps holds with `STALL=seed`; `spell` prints the ball on the carrier line; `high_balls` splits free and
contested passes.

## Short-pass losses and conditional targets (2026-10-29)

Asked for: the short-pass losses (short ground passes lost about twice as often as real). Midway through, the user
pointed out the pattern of the earlier rounds: every screw tightened here loosened one there, because the targets
were single means over very different matches (long high balls to a strong centre forward, deep blocks countering
with long low balls, two sides that never play a high ball). From then on:
- Physics, perception, execution and the AI's pricing are calibrated only on **situation-conditional** targets ("in
  this geometry, what happens"), and only after checking in StatsBomb that the target holds across playing styles.
- Volume and mix stats (passes, long-ball share, lofted volume, tackles, goals) are **plausibility checks**: an
  archetype squad with matching instructions has to land inside the real spread of its style (`styles` bench below).

**Footing.** StatsBomb's end location of an incomplete pass is where the ball was touched or went out, so the
`intercepts` bench ends a lost pass at the first opponent's touch too. Real ground passes lost per match (× 1.48) by
touch distance from the passer 0–2 / 2–5 / 5–10 / 10–15 / 15–20 / 20–30 / 30–40 m: 4.8 / 14.8 / 13.9 / 11.6 / 8.6
/ 7.9 / 2.3 (`sb_blocked.py`). The engine lost about twice as many at every distance (0–5 m 35.9 against 19.6,
20–30 m 24.0 against 7.9). Ball speed is not the cause: completed ground passes take 0.84 / 1.22 / 1.56 / 2.16 s
over 0–10 / 10–20 / 20–30 / 30–45 m, real 0.90 / 1.25 / 1.71 / 2.21 s (`sb_ground_duration.py`).

**Kept.**
1. Receiver steering on ground passes (ATT-24, `positioning::receive_steering`). The receiver steers onto the ball's
   line and stops across it, moving along it at the speed that brings him to his reception point with the ball.
   Before, he ran at the point; coming from the side he crossed the line before the ball arrived. In lost passes 98 %
   of the receivers were meeting the ball and 82 % had been on its line early, with the defender cutting across a
   median 2.4 m in front of them.
2. Look again (ATT-05, `Simulation::decide_reception`). The ATT-06 first-time decision is taken at the release and
   once more after the receiver's reaction time, with the same decision seed.

Lab 40: pass completion 78–79 → 82 %, goals 3.05 → 3.00, tackles 171 / 181 → 173 / 182 per team. Gap table
(`layers`, 16 matches): 68 v 47 5.00–0.31, 33.2 v 5.2 shots (was 5.75–0.25, 32 v 2.9); 58 v 47 2.50–0.38, 19.4 v
6.5 (was 2.56–0.56, 15.4 v 5.6); 68 v 68 1.88–1.75.

**Tried and dropped.**
- IND-08 `pass_cue_s` 0.2 / 0.1 / 0 s. Completion by the nearest opponent to the line stayed at 66 / 76 / 84 / 88 /
  96 % (lane 0–1 / 1–2 / 2–3 / 3–5 / 5+ m) in all three. A later cue only made the AI pick narrower lanes (958 → 1059
  → 1177 ground passes of 5–40 m per match) and overrate first-time passes (81 % predicted against 72 % at 0 s). Kept
  at 0.2 s.
- A sprint penalty on an interceptor's touch (an opponent running flat out across the ball's path gets a foot to it
  less often). Interceptors touched passes at a median 6.5 m/s, receivers at 1.7 m/s. Dropping the touch chance by
  up to 0.3 / 0.5 / 0.7 at a sprint barely moved completion by pressure on the passer. Instead the first-time share
  under tight pressure rose from 33 to 41 to 51 %, against a real 7 %: the AI used the now-cheaper first-time passes
  more. Removed.

**Conditional targets** (`intercepts` bench; scratchpad scripts `sb_press_compl.py`, `sb_ft_press.py`,
`sb_lane_u.py`; 166 StatsBomb 360 matches; ground passes of 5–40 m; the passer's surroundings within 8 m visible):

| Nearest opponent to the passer at release | 0–1.5 m | 1.5–3 m | 3–5 m | 5–8 m | 8+ m |
|---|---|---|---|---|---|
| share of passes, engine / real | 7 / 13 % | 21 / 29 % | 30 / 28 % | 19 / 18 % | 14 / 13 % |
| completion, engine | 83.9 % | 86.2 % | 86.6 % | 90.7 % | 95.7 % |
| completion, real | 87.9 % | 92.3 % | 95.1 % | 97.1 % | 99.1 % |
| first-time share, engine | 32.6 % | 24.5 % | 20.6 % | 8.0 % | 0.8 % |
| first-time share, real | 6.6 % | 8.1 % | 9.6 % | 6.9 % | 3.5 % |
| first-time completion, engine / real | 61 / 84 % | 72 / 89 % | 77 / 92 % | 79 / 95 % | 73 / 98 % |

- By direction: lateral 5–10 m passes complete 75 % (real 93 %), backward 10–20 m ones 93 % (real 99 %).
- Deflected passes that the passing team still kept (StatsBomb logs those as complete) are 0.6 % of passes, so the
  gap is not a definition issue.
- The AI's success estimate holds for free passers (`intercepts`, by the AI's p): p ≥ 0.95 completes 98 %, 0.85–0.95
  93 %, 0.7–0.85 82 %, 0.5–0.7 70 %. A free passer's loss comes from what he chooses, not from what happens to the
  ball. New bench `free_risk`: of the passes free carriers choose, 15 / 43 / 69 % (own / middle / final third) have
  p < 0.85. Most of them are first-time decisions taken while the ball was on its way, by a receiver forecast to be
  pressed on arrival (ATT-06). 45 % of those risky choices were not the layer-1 best but softmax picks among
  options within ≈ 0.001–0.005 utility.

**Diagnosis.** The remaining gap is a choice made under pressure. The AI escapes pressure with first-time passes 3–5×
as often as real players do, and those passes complete about 20 points less often. The choice is consistent with the
engine's physics: a pressed receiver who controls the ball loses it before his next pass 27–35 % of the time,
mostly tackled after 1–2 s, and tackles run at ≈ 175 per team. Real players keep the ball under pressure with a
first touch away from it and the half-turn, which is the part of ATT-05 still deferred to M6. The next lever is
therefore pressed receptions, measured against the table above, not the pass physics.

### Style invariance of the targets

Team and style were joined onto the 360 passes (`sb_team_styles.py`: events of the 166 matches, 332 team-matches).
The team-matches were split into terciles on four axes: the passing team's directness and possession share, and the
defending team's PPDA and defensive-action height. Then each target was recomputed per tercile
(`sb_invariance.py`; spread across terciles, sampling error ± 0.1–1.6 points):
- **Invariant:**
  - completion by pressure on the passer: 2–3 points at 0–3 m, ≤ 1.2 above 5 m;
  - completion by distance of the nearest opponent to the line: ≤ 2 points;
  - first-time share of ground passes: 6.8–8.3 % on every axis;
  - 10–20 m ground completion: 94.2–96.6 %.

  The spread is smaller than the engine's gap and partly reflects quality: possession sides complete a little more
  at every pressure.
- **Weakly style-dependent:** first-time completion (83–90 % under pressure) and the passer-pressure mix (39–46 % of
  passes with an opponent within 3 m).
- **Style-dependent:** completion of high 30–45 m passes (45–55 %; direct sides lowest), the share of passes ≥ 30 m
  (8–16 %) and the high-pass share (9–19 %). These are plausibility checks, not targets.

The calibration table above, by class:
- **Conditional targets:**
  - goals per xG and its splits; save % by band; keeper depth by band; unblocked shots on target;
  - shots blocked by band; take-on success rate; the receiver's loss before his next pass by pressure;
  - lofted hang time; the AI's p against completion;
  - completion by pressure on the passer and by lane; first-time share by pressure; ground-pass durations;
  - interceptor geometry; GK distance to the goal line and to the defensive line.
- **Phase-conditional, with a team spread:** mid-block and low-block length, width and line, and the in-possession
  shapes. These are what the sourced layer-1 defaults produce with no instructions, so they must fall within the
  published team range (e.g. 22.5–29.1 m), not on its mean.
- **Style-dependent, plausibility per archetype:**
  - goals and shots per match; shots from outside the box; open-play passes; the 30–45 m share; switches;
  - lofted volume and its completion; tackles, turnovers and changes of possession; possession; overall pass
    completion;
  - the weak-foot shares (these depend on the squad); support distances; the passer-pressure mix;
  - pressers per press; counter-press and counter-attack figures; one-touch goals and goals from 1–5 passes;
  - off-ball speed.

### Style archetypes against real style clusters

**Real clusters.** `sb_style_clusters.py` runs k-means (k = 4) on the 332 team-matches. The features are directness
(share of open-play passes ≥ 30 m), high-pass share, possession share, log PPDA, and the height of the team's tackles
and interceptions. The four clusters line up with the styles named above:

| Cluster | Teams most often in it | Directness | High share | Possession | PPDA | Def. height | Open-play passes | First-time share |
|---|---|---|---|---|---|---|---|---|
| C0 possession | Spain, England, Germany | 0.088 | 0.106 | 0.63 | 9.6 | 49.6 m | 599 | 0.087 |
| C1 deeper block | Switzerland, France, Croatia | 0.110 | 0.140 | 0.47 | 15.0 | 39.2 m | 467 | 0.104 |
| C2 pressing | Serbia, England, Denmark | 0.127 | 0.171 | 0.50 | 9.6 | 46.5 m | 445 | 0.101 |
| C3 direct | Wales, Poland, Slovenia | 0.170 | 0.254 | 0.35 | 14.1 | 41.0 m | 295 | 0.136 |

Values are medians per real match. (The first-time share here counts every open-play pass within 0.5 s of its
receipt, headers included, so it runs higher than the ground-pass share above.)

**Archetypes.** The new `styles` bench defines four archetypes, each a squad overlay plus layer-2 instructions. The
overlays trade attributes roughly evenly, so that quality stays about the same.
- **Possession:** passing, first touch, composure, vision and technique +12, physical −12. Directness −1, tempo −0.5,
  full width, high block, pressing 0.7.
- **Deep block + counter:** forwards' pace +15, defenders' marking and positioning +10, passing −10. Low block, no
  pressing, directness +0.5, tempo +0.7.
- **High press:** stamina, work rate, aggression and acceleration +12, composure and technique −12. High block,
  pressing 1, tempo +0.5.
- **Direct to a target man:** the striker becomes 194 cm, with strength and heading of 88 or more and pace −15.
  Wide players get crossing +12. Directness +1, mid block, pressing 0.3.

Each archetype plays both as Northbridge (4-3-3) and as Harbour (4-4-2) against the other default side. Two
head-to-heads complete the bench: possession against the deep block, and the target man against the high press.
`STYLES_OUT` writes every team-match out; `styles_vs_real.py` and `sb_style_clusters.py` then place each one in its
nearest real cluster.

First run (6 seeds per pairing, counts ÷ 1.48):

| Archetype | Directness | High share | Possession | PPDA | Def. height | Passes | First time | Shots | Nearest real cluster |
|---|---|---|---|---|---|---|---|---|---|
| default | 0.297 | 0.115 | 0.51 | 3.4 | 43.7 m | 551 | 0.26 | 11.0 | C2 (60 of 60) |
| possession | 0.176 | 0.085 | 0.48 | 2.8 | 50.1 m | 533 | 0.24 | 15.1 | C2 (18 of 18; wanted C0) |
| deep block + counter | 0.358 | 0.125 | 0.52 | 4.1 | 37.9 m | 564 | 0.28 | 8.4 | C3 15, C2 3 (wanted C1) |
| high press | 0.267 | 0.100 | 0.46 | 2.7 | 49.9 m | 516 | 0.30 | 14.0 | C2 (18 of 18) |
| direct to a target man | 0.391 | 0.147 | 0.51 | 3.4 | 44.1 m | 571 | 0.29 | 9.7 | C3 13, C2 5 |

- **Right direction:**
  - the possession side is less direct, plays fewer lofted balls, defends higher (≈ C0's 49.6 m) and shoots more;
  - the deep block defends at 37.9 m (C1 39.2 m) and shoots less;
  - the high press defends high with the lowest PPDA;
  - the target-man side is the most direct with the most lofted balls.
- **Engine-wide offsets, outside every real cluster:**
  - directness 0.18–0.39 (all archetypes at or above the real 92nd percentile);
  - first-time share 0.24–0.30 (real 100th percentile);
  - PPDA 2.7–4.1 (real 0th percentile: ≈ 175 tackles per team).
- **No response:**
  - possession share stays at 0.46–0.52 and open-play passes at ≈ 520–570 in every archetype. Real possession sides
    reach 0.63, direct ones 0.35 and ≈ 300 passes;
  - the target-man side's lofted share (0.147) stays below C3's 10th percentile (0.196).
- **Conditional stats:** in every archetype completion by pressure on the passer is 75–77 / 81–84 / 83–88 / 88–91 /
  93–95 % (counting passes into space too), and the first-time share is 27–30 % under 3 m. Style does not move them,
  as in StatsBomb, but they sit below real everywhere.

All three offsets, and the possession share that does not separate by style, point to the same place as the
conditional table: the ball is lost too easily under pressure. Possessions are short for every style (≈ 275 changes
of possession per match against ≈ 162), so the share of the ball cannot reflect how a team plays. The next round
targets pressed receptions and ball retention (ATT-05's first touch away from pressure, shielding). It is measured
on the conditional table and checked on this bench: possession should separate by style, directness and the
first-time share should fall into the real range.

## Pressed receptions (2026-10-29)

Asked for: the pressed receptions, where the short-pass round ended. The targets are StatsBomb 360 receptions of
ground passes, by the nearest opponent at the receipt (`receptions` bench; scratchpad scripts `sb_receipt360.py`,
`sb_receipt_carry.py`; 66 880 receptions with 8 m around the receiver visible). Across terciles of directness and
possession share of the receiving side, and of PPDA and defensive height of the defending side, the shares move by
2–5 points at most, so they are conditional targets.

| Nearest opponent at the receipt | 0–1.5 m | 1.5–3 m | 3–5 m | 5–8 m | 8+ m |
|---|---|---|---|---|---|
| first time, engine before / now / real | 62 / 68 / 24.5 % | 60 / 64 / 24.7 % | 50 / 53 / 16 % | 21 / 22 / 6.5 % | 13 / 14 / 2.2 % |
| lost before the next pass or shot, of all receptions, before / now / real (+ fouls won) | 24 / 16 / 20 (+7.1) % | 19 / 11 / 11 (+3.5) % | 17 / 11 / 6.3 (+1.8) % | 15 / 7.6 / 2.6 (+0.9) % | 6.1 / 3.0 / 0.8 (+0.2) % |
| lost, of the controlled receptions, before / now / real | 62 / 48 / 27 % | 48 / 30 / 15 % | 33 / 23 / 7.5 % | 18.5 / 9.7 / 2.7 % | 7.0 / 3.6 / 0.8 % |
| median time on the ball when kept, before / now / real | 2.26 / 1.62 / 0.82 s | 2.04 / 1.48 / 0.77 s | 1.76 / 1.22 / 0.94 s | 0.98 / 0.86 / 1.32 s | 0.76 / 0.74 / 1.82 s |
| losses per second on the ball after control, before / now / real | 0.29 / 0.31 / 0.28 | 0.18 / 0.16 / 0.14 | 0.14 / 0.13 / 0.056 | 0.088 / 0.064 / 0.016 | 0.041 / 0.028 / 0.003 |

"Before" is 12 matches at the start of the round, "now" 16 matches. The engine has no fouls, so real receptions
that end in a foul won are listed apart.

**Kept: dribble directions share one softmax weight** (`on_ball::softmax`). The carrier tries seven dribble
directions, and each had its own weight, while the ways to pass to one team-mate and the shot variants already shared
one. In a near tie, carrying therefore had several entries against each pass. New `carry_cal` table: of the dribbles
and take-ons chosen within 3 m of an opponent (1179 per match), 44 % had a better pass on layers 1 + 2, so
personality or the softmax picked them. The dribble group now gets the weight of its best direction, shared in proportion to the directions'
own weights, as the passes to one team-mate do (`tests/personality.rs`).
- Chosen per match (`carry_cal`, 12 matches): dribbles 1969 → 1172, take-ons 409 → 231, holds 738 → 1029. The AI's
  pricing is unchanged: dribbles predicted to keep the ball 0.89 (kept 0.94), take-ons 0.76 (kept 0.85).
- Lab 40: goals 3.00 → 2.50, shots 19.4 / 18.3 per team, passes 922 / 1014 (81 / 82 %), tackles 173 / 182 → 122 /
  126 per team. `duels`: successful take-ons ≈ 37 → 20 per team.
- Gap table (`layers`, 16 matches): 68 v 47 4.81–0.31, 37.4 v 5.1 shots; 58 v 47 2.12–0.44, 19.7 v 8.1; 68 v 68
  1.31–1.62.
- Unchanged: completion by the pressure on the passer 84 / 86 / 87 / 90 / 96 % and the first-time share there 32 /
  24 / 23 / 9 / 1 % (`intercepts`). On `styles`, possession still stays at 0.46–0.52 and the first-time share at
  0.23–0.30 for every archetype; PPDA rises to 3.0–4.7.

**Tried and dropped.**
- `att25_pressed_loss` 0.3 → 0.6 / 1.0, so that a carry or control ending pressed is priced as losing more. Lost of
  controlled fell to 56 / 40 / 22 / 13 / 4 % and 51 / 26 / 16 / 7 / 3 %. The first-time share at 0–1.5 m rose to
  79 / 87 % (real 24.5 %): the AI escaped through first-time passes instead.
- A touch-space rule ([FA](https://learn.englandfootball.com/articles-and-resources/coaching/resources/2023/How-to-encourage-running-with-the-ball-in-football):
  big touches when space is available, but "if touches are too big, it's easy to lose control"). No effect beyond
  noise; removed.
- Carry time with the turn in `dribble_keep` (the carrier reaches a path point no sooner than `time_to_reach`
  allows). Lost of controlled fell to 41 / 33 / 20 / 11 / 5 % and tackles to 226 per match, but only because the AI
  turned pessimistic: dribbles predicted 0.79–0.82 against 0.95 kept, take-ons 0.52 against 0.83. It carried less
  (dribbles 1969 → 1230 per match). The kept rate hardly depends on the turn: 0.95 / 0.96 / 0.94 / 0.91 for turns of
  0–0.1 / 0.1–0.3 / 0.3–0.6 / > 0.6 s. Scaling the race term by 0.35 restored the predictions (0.88 against 0.92)
  and with them the losses (54 / 47 / 33 / 21 / 7.6 %). Reverted.
- All personality biases off (egotism, flair, bravery): lost of controlled 54 / 34 / 26 / 13 / 4 %. That is part of
  the excess, but it is layer 3 working as designed; kept.
- IND-03 `ind03_last_ditch_s` 0.3 → 0 (no challenge against the odds when about to be beaten): tackles 122 / 126 →
  97 / 107 per team, lost of controlled hardly moved (46 / 30 / 22 / 10 / 3.3 %), goals 2.50 → 3.45. Kept at 0.3 s.
  `duel.tackle_loose_margin` 0.3 → 0.6: no change.
- ATT-06 as a two-step choice: all first-time passes share one weight against "control". The first-time share under
  5 m fell from 67 / 64 / 53 % to 55 / 50 / 44 %, but the extra receivers who controlled were tackled: lost of all
  receptions rose from 16 / 11 / 11 % to 21 / 15.5 / 11.4 %. Not kept.

**Diagnosis.**
- **The AI prices what the engine does.** For "control" after an ATT-06 decision it implies a loss of 0.36 / 0.28
  / 0.17 / 0.06 by band, against 0.45 / 0.28 / 0.22 / 0.12 actual. For dribbles it predicts 0.89 against 0.94 kept.
  First time or control and carry or pass are rational choices given the engine's outcomes. Every decision-side
  lever above that cut the losses did so by making the AI misjudge them.
- **Under pressure the loss per reception is about real.** Under 3 m the engine loses 16 / 11 % of all receptions,
  real players 20 / 11 % plus 7 / 3.5 % fouls won. Per second on the ball after control the 0–1.5 m rate matches too
  (0.31 against 0.28). The engine keeps the ball longer there (mean 1.54 against 1.07 s), and it escapes 2.7× as often
  first time, with first-time passes completing 68 % against 84 %.
- **In space the carry itself loses the ball.** From 3 m the losses per second on the ball are 2–10× real while time
  on the ball is shorter. At 8+ m, 52 % of the losses are tackles, 36 % heavy dribble touches and 12 % balls carried
  out. Tackles still run at ≈ 248 per match against ≈ 50 real. The commit rules are not the cause: the "why
  committed" split (`tackles`, new `TeamStats::tackle_reasons`) is IND-03 last ditch 78, loose touch 69, duel odds
  with the ball nearer the defender 78, aggression 15 per match, and switching any one off leaves the reception
  losses where they were.
- **Time on the ball runs the wrong way.** The engine keeps it longest under pressure and passes soonest in space
  (median 0.74 s at 8+ m, real 1.82 s): at the first decision after the settle a free receiver passes half the time.
  The continuation half-life does not move this (it saturates, see "Continuation value and settle time").

Next: the carry in space (why a free carrier is tackled or loses a touch 10× as often per second as real) and the
execution of pressed passes (first-time completion 68 % against 84 %). Not the decision pricing.

Benches extended: `receptions` (losses per second on the ball, the first decision after control, implied against
actual loss after "control"), `carry_cal` (ATT-25 chains, turn-time bins, why a dribble was chosen), `tackles` (why
committed, carry-tackle geometry).

## Carry in space (2026-10-29)

Asked for: the carry in space, where the pressed-receptions round ended (losses per second on the ball 2–10× real
from 3 m). The targets are the same StatsBomb 360 receptions of ground passes as in "Pressed receptions".

**Bench correction first.** `receptions` counted a first-touch Miscontrol as a loss even when the receiver got the
ball back. StatsBomb logs a miscontrol only when it loses the ball, so a heavy touch now counts as lost only if the
opponents control it, or get the restart, before he does. Last round's "36 % heavy dribble touches at 8+ m" was this
artefact. In space the engine's heavy touches almost never lose the ball.

**New bench `space_losses`.** It follows controlled receptions of ground passes with the nearest opponent ≥ 5 m at
the control, until a pass or shot (kept), a tackle that is not Beaten, a miscontrol, an opponent's control, or a
restart. It reports:
- the loss rate and losses per second, and the cause;
- the nearest opponent and the category of the last decision before the loss;
- the tackle reasons and the tackler's side;
- the chosen option's softmax probability;
- the share of carrying time by the ball's gap to the carrier and by how overdue his decision is;
- with `TRACE=n`, a 0.1 s trace with the options at each decision.

**Diagnosis.** A free carrier knocks the ball on at speed (`dribble_touch`: lead `dribble_lead(pressure)` up to
2.5 m), and it then runs 0.8–1.5 m ahead of him for 1.5–2.7 s. That is beyond `control_radius` (0.75 m), and the
carry branch needed the ball at his feet both to decide and to touch. So he could neither re-decide nor touch it
while an opponent closed from 6–8 m at 8–10 m/s and tackled. The ball was at his feet only 51 % of the carrying
time. Within 1.5 m his decision was overdue 63 % of that time. On the tackle side, the per-tick check let a defender
act on a ball nearer to him than to the carrier the moment it was so. 78 tackles per match were "odds, ball nearer
him", and many of them came within a few ticks of a knock-on.

**Kept.**
- `player.run_reach_s` 0.1: running onto the ball, he can play it (decide, pass) within `control_radius` +
  0.1 s × his speed towards it; the next dribble touch still waits until it is at his feet. 0.15 and 0.2 gave
  nothing more (the effect saturates).
- The settle catch-up (`settle_catch_s` 0.4) now applies during all carrying: he closes the gap a knock-on left
  instead of trailing it at dribbling pace.
- `duel.tackle_cue_reaction` 1.0: a defender acts on a ball that has come nearer to him than to the carrier (the
  +0.3 odds and the loose-ball commit) only once it has been so for his reaction time (0.18–0.38 s). Pure mechanics,
  like `pass_reaction`. At 0.5 the effect was about half.
- `ai.decision_noise_pressure_bands` 3: the held misjudgement (`decision_noise_hold_s`) is drawn afresh when the
  pressure on him moves to another third of its range. The traces showed carriers dribbling straight on at a softmax
  probability of 0.07–0.11 while "hold / shield" rose to 0.53 and the defender closed from 6 m to 1 m. Of the
  losses after carrying, 64–68 % came from options picked at a probability below 0.15, and 57 % from options below
  half the likeliest.

| Nearest opponent at the receipt | 0–1.5 m | 1.5–3 m | 3–5 m | 5–8 m | 8+ m |
|---|---|---|---|---|---|
| lost, of the controlled receptions, before / now / real | 45 / 32 / 27 % | 28 / 23 / 15 % | 22 / 13 / 7.5 % | 8.9 / 5.6 / 2.7 % | 2.5 / 1.5 / 0.8 % |
| lost before the next pass or shot, of all receptions, before / now / real (+ fouls won) | 15 / 9.8 / 20 (+7.1) % | 10 / 8.6 / 11 (+3.5) % | 10.5 / 5.9 / 6.3 (+1.8) % | 6.9 / 4.1 / 2.6 (+0.9) % | 2.1 / 1.2 / 0.8 (+0.2) % |
| losses per second on the ball after control, before / now / real | 0.27 / 0.18 / 0.28 | 0.15 / 0.14 / 0.14 | 0.12 / 0.081 / 0.056 | 0.058 / 0.042 / 0.016 | 0.019 / 0.013 / 0.003 |
| first time, now / real | 69 / 24.5 % | 63 / 24.7 % | 56 / 16 % | 27 / 6.5 % | 19 / 2.2 % |
| median time on the ball when kept, now / real | 1.52 / 0.82 s | 1.32 / 0.77 s | 1.16 / 0.94 s | 0.82 / 1.32 s | 0.74 / 1.82 s |

"Before" is the corrected bench at the start of the round (12 matches), "now" 16 matches. In `space_losses` (12
matches) lost fell 7.6 → 4.1 % at 5–8 m and 2.1 → 1.2 % at 8+ m. The ball is at his feet 55 % of carrying time, and
within 1.5 m his decision is overdue in 40 % of that time instead of 63 %.
- Tackles (`tackles`, 12 matches): 247 → 185 per match. Reasons per match: last ditch 82, loose touch 71, odds with
  the ball nearer him 78 → 10, controlled 4, aggression 15, shot 3.
- Lab 40: goals 2.50 → 3.03, shots 19.4 / 18.3 → 22.2 / 21.0 per team, passes 922 / 1014 → 986 / 1078 (81 / 82 %),
  tackles 122 / 126 → 90 / 98 per team (58 / 56 % won).
- Gap table (`layers` 16, `SIDES=1 EGO=0 FLAIR=50`): 68 v 47 4.94–0.44, 35.8 v 5.0 shots; 58 v 47 2.31–0.69, 21.0
  v 7.8; 68 v 68 1.19–0.81, 19.9 v 19.8.
- `styles`: PPDA 3.0–4.7 → 3.5–5.4 (real terciles < 9.8 / 9.8–13.5 / > 13.5). Possession still 0.46–0.52 and
  the first-time share 0.24–0.30 for every archetype.
- The AI's pricing still matches: "control" implied 0.36 / 0.29 / 0.16 / 0.06 against 0.31 / 0.22 / 0.14 / 0.07
  lost; dribbles predicted to keep the ball 0.89 against 0.95.

**Tried and dropped.**
- `decision_noise_hold_s` 2 → 1 / 0.5. Lost fell (8+ m 1.7 → 1.3 / 1.2 %), but only because time on the ball
  fell (mean 1.05 → 0.95 / 0.89 s). Losses per second stayed at 0.014 / 0.051 per s, so free receivers passed sooner
  still, the wrong way.

**Remaining.**
- In space, losses per second are still 2.6× real at 5–8 m and 4× at 8+ m. At 8+ m the tackles are IND-03 last
  ditch (49 %) and loose touches seen in time (42 %).
- The knock-on lead is set from `pressure_at`, which uses distance only, so a defender sprinting in from 6.5 m counts
  as no pressure and the ball goes 2.5 m ahead.
- Real defenders about to be beaten in space often foul; the engine has no fouls.
- Under 1.5 m the engine now loses less than real (9.8 % against 20 % plus 7 % fouls won), because first time
  (69 % against 24.5 %) still takes most pressed receptions out of the duel.
- Time on the ball still runs the wrong way: free receivers pass at the first decision (median 0.74 s at 8+ m,
  real 1.82 s).
- Tackles are still ≈ 3.7× real (≈ 50, or ≈ 80 attempts counting dribbled-past).
- `shots`: 71 % of shots come from outside the box at the start of the round and 73.5 % now, against ≈ 32 % real.
  That predates this round; the calibration table's ≈ 41 % was out of date.

Next: free receivers' time on the ball and shot selection (outside-box share), then the knock-on lead against a
closing defender.

## Shot selection and hold pricing (2026-10-30)

At the start of the round 73 % of the engine's shots came from outside the box (real ≈ 32 %), and free receivers
passed at their first decision. The new `shot_share` bench takes every pass and shot by its distance from the goal and
counts the shot's share of them. For each band it shows:
- free and pressed carriers;
- the decision the shot came from: layer-1 best, best after layers 2+3, chosen;
- the support around the carrier;
- the spread between players.

The real values (StatsBomb open data, footed open play; scripts in the scratchpad):

| | engine at the start | real |
|---|---|---|
| shot share of passes + shots at 0–11 / 11–16.5 / 16.5–22 / 22–30 / 30–40 m | 93 / 75 / 55 / 32 / 4.4 % | 85.6 / 45.7 / 23.9 / 7.9 / 0.2 % |
| 22–30 m, by team-mates in the box 0 / 1 / 2 / 3+ | | 8.0 / 9.3 / 7.9 / 9.5 % (no dependence) |
| per player at 16.5–30 m (≥ 25 actions), p10 / p50 / p90 / max | 31–45 % for the six attackers | 2 / 14 / 19 / 24 % (32 players); wingers and AMs 11.4, midfielders 15.2, forwards 15.3 % |
| 22–30 m: nearest team-mate < 5 / 5–10 / 10–15 / 15+ m (share of actions) | 10 / 31 / 41 / 18 % | 17.7 / 60 / 17.4 / 4.9 % (360 data) |
| 22–30 m: team-mates within 15 m 0 / 1 / 2 / 3+ | 18 / 41 / 36 / 5 %; shot share 24–39 % in every bin | 4.9 / 16.6 / 24.3 / 54.2 %; shot share 3–11 %, lowest (2.8–3.2 %) with 0–1 team-mates near |
| free receiver (nearest opponent ≥ 5 m): receipt → next pass or shot, p25 / median / p75 | median ≈ 0.8 s | own third 0.95 / 1.55 / 2.76 s, middle 0.93 / 1.47 / 2.48 s, final third 0.90 / 1.45 / 2.40 s |

**What the decisions show.**
- With every player at egotism 0 and flair 50, the 22–30 m share is 15.2 % instead of 32 %. Personality doubles
  it, but layer 1 alone already takes twice the real share.
- A typical 22–30 m decision in a trace:
  - layer 1: the shot ≈ xG (0.025–0.03) + the ATT-01 tie-break ≈ 0.006 − the miss cost 0.0024;
  - the keep options (hold, dribble) 0.022–0.045;
  - the passes poor (p 0.3–0.65);
  - personality: + 0.010–0.015 on the shot, − 0.007 on the passes.
- So the shot competes at near-equal value, and the softmax gives it a large share.

**Kept.**
- **ATT-01 shot tie-break as a share of the shot's own xG** (`on_ball::shots`; see the catalog). As a share of the
  local value scale it was ≈ 0.006 at 25 m, 30 % of a long shot's worth. 22–30 m 31.7 → 28.7 %.
- **`egotism_shot_factor`** (`PersonalityConfig`, 1.5, before hard-coded). Lowering it moves the level only a little:
  1.0 / 0.5 give 24.4 / 23.2 % at 22–30 m. It also flattens the spread between players, max / median 1.15 / 1.1
  against real 1.7, while 1.5 gives 1.68. The spread is right and the level is layer 1's, so 1.5 stays. (Replaced on
  2026-11-01 by `egotism_shot_share`, a share of the shot's own xG; see "Watch round".)
- **`hold_loss` 0.6 → 0.05** (`AiConfig`, before hard-coded). The hold option priced the ball lost before the next
  decision (0.3 s away) at up to 0.6 × pressure. `hold_cal` measures that under full pressure 97.5 % of holders still
  have it at the next decision, against 72 % priced. ATT-25's `pressed_loss` already charges the loss over the rest of
  the spell, so the old price counted it twice. Now 0.97 is priced against 97.5 % kept.

**Results** (`shot_share` / `receptions` 12 matches, `layers` 16, lab 40):
- Shot share by band: 93 / 75 / 55 / 32 / 4.4 → 95 / 75 / 62 / 28 / 4.3 %. The 16.5–22 m band has ≈ 80 shots per
  run, so it is noisy (47–62 % across the variants); 22–30 m is the reliable band.
- Open-play shots 41 → 36 per match; inside the box 26.5 → 30 %.
- Receptions:
  - first time 37.6 → 34.5 %;
  - time kept 0.80 → 0.84 s;
  - lost 6.7 → 7.3 %;
  - free receivers (8+ m) unchanged at 0.74 s.
- `styles`: first-time share 0.24–0.29 → 0.21–0.27 (real clusters 0.09–0.14). The other columns are unchanged.
- Gap table, before → after:

  | | goals | shots |
  |---|---|---|
  | 68 v 47 | 4.94–0.44 → 3.31–0.38 | 35.8 v 5.0 → 35.3 v 5.9 |
  | 58 v 47 | 2.31–0.69 → 2.12–0.62 | 21.0 v 7.8 → 20.8 v 8.1 |
  | 68 v 68 | 1.19–0.81 → 0.69–1.25 | 19.9 v 19.8 → 17.4 v 21.1 |

  xG is unchanged (3.60 → 3.71, 1.69 → 1.71, 1.45 / 1.45 → 1.41 / 1.45).
- Lab 40: 2.17 goals from 2.61 xG; with the tie-break alone, 2.67 from 2.63. Chance creation is unchanged and the
  difference is conversion noise.

**Tried and dropped.**
- `value_base` 0.0097 (all possession values up). 22–30 m fell to 27 % (20 % with the tie-break off), but the side
  effects went far beyond the shot:
  - pass completion 84 → 88 %;
  - possessions 194 → 151;
  - shots in the 68 v 47 gap match 35.8 → 24.8.
- A value curve fitted to the StatsBomb "xG still to come" table. The shot shares got worse: 68 / 35 % at 16.5–22 /
  22–30 m. A lower value near the box makes the shot relatively better, and the final third is reached less often.
- Continuation half-life 3 s and 10 s. Free receivers' time on the ball stayed at 0.76–0.84 s, because the
  continuation saturates.
- `egotism_shot_factor` 1.0 / 0.5: see above.

**Remaining.**
- **Final-third support** is the structural cause of the long-shot level. At 22–30 m engine carriers have 3+
  team-mates within 15 m in 6 % of actions against 54 % real, and none in 18 % against 4.9 %. Over the whole pitch
  support is close to real (team-mates within 10 / 15 m 1.02 / 2.15 against 1.09 / 2.48), so the gap is in
  final-third positioning: ATT-08 / 09 / 10 in `positioning::score_attack`, where the shape cost pulls players to
  the anchor and ATT-10's crowding radius is 8 m. Even with support, though, the engine shoots 3× the real share in
  every bin.
- **Long shots return less than their price.** At 22–30 m two 40-match `shots` runs (before and after this
  round) scored 8 and 12 goals against 15.6 and 16.0 xG: 0.63 together, against 0.81–1.17 in the box. 91–95 % of
  the on-target shots there are saved, and outside-box conversion is 1.8–2.5 % against 3.1 % in StatsBomb footed open
  play. The AI prices these shots from the xG model, so it overrates them by about a third. Whether the keeper or the
  finishing is off has to be settled before the price is matched to the outcome.
- **Free receivers** still pass at the first decision. At 5+ m the layer-1 best is a pass in 58 % of first
  decisions, with a median gap of +0.0008 over the best carry. Every receiver is its own alternative in the softmax,
  while shots and dribbles each share one weight. Two candidates:
  - group the passes the same way;
  - an ATT-06 "turn when free" carry.
- **Long passes.** Per match the engine plays 566 passes of 30–45 m (forward 316, back 205) against 102 real ×1.48
  (73.5 / 18.2): back passes over that length are 11×. A separate structural topic.

Next: final-third support.

## Final-third support and line depth (2026-10-30)

The shot-selection round left one structural gap. With the ball 22–30 m from goal, real carriers have team-mates near
them and in the box, and the engine's mostly do not. The new `final_third` bench records the following at each
open-play pass:
- the shape around the passer;
- per role at 22–30 m: anchor, target and position against the ball;
- the defending back line's target, block line and label;
- the most advanced team-mate's gap to the offside line, split into anchor, target and position;
- a line-depth table by the ball's distance to the goal line (central or wide, passer free or pressed).

The real counterparts are the scratchpad scripts `sb_ft_shape.py` and `sb_line_depth.py`, run on the StatsBomb 360
frames.

| at an open-play pass, ball 22–30 m from the goal centre | engine at the start | engine now | real |
|---|---|---|---|
| team-mates within 15 m ahead / level or behind | 0.31 / 0.94 | 0.38 / 0.88 | 1.40 / 1.20 |
| team-mates in the box | 0.19 | 0.26 | 1.70 |
| most advanced team-mate / deepest outfield opponent, m from the goal line | 22.6 / 15.8 | 21.6 / 14.8 | 13.4 / 12.1 |
| opponents within 15 m / in the box | 6.29 / 1.39 | 6.24 / 1.89 | 5.10 / 3.30 |

**What the real data shows.** The band means mix wide and central balls. Binned by the ball's distance to the goal
line, three clean relations appear:
- The deepest defender stands at a drop share of 0.39–0.46 of the ball's distance with the passer free and 0.33–0.39
  with an opponent within 3 m of him. The line steps up under pressure, as BLK-02 says.
- The most advanced attacker is 1.0–1.5 m behind the deepest defender at every distance from 10 to 50 m.
- Team-mates in the box fall from 2.4 (ball 10–14 m) through 1.7 (18–22 m) to 0.7 (26–30 m).

**What the engine showed.**
- The drop share was 0.29–0.33, so the line stood 2–4 m high.
- The most advanced attacker was 5–7 m behind the deepest defender, in the middle third as well.
- The gap of the most advanced attacker (ball 22–46 m) splits into:
  - the anchor 1.6 m short of the offside line (onside margin, slot depth);
  - the target 0.5 m behind the anchor;
  - the player 2.8 m behind his target.

  Every role trails its target by 2–3 m. Positional urgency is first order: full speed only at
  `position_full_speed_dist` (20 m), a time constant of ≈ 2.4 s. A line that keeps moving is therefore followed at a
  distance.

**Kept.**
- **ATT-14 for strikers** (`positioning::score_attack`; replaced in the next round by ATT-26, see "Pin the line or
  offer short"). DFB-SCHOEN names a striker in a cover shadow
  coming short as the error, and the correction is to go "so tief wie möglich … um die gegnerische Verteidigung zu
  binden". For role ST three terms are off: the shadow penalty, the ATT-15 bonus, and the ATT-08 lane bonus for points
  more than 1 m short of the anchor. Strikers 0.0 → +1.3 m ahead of the ball at 22–30 m; small on its own.
- **`blk02_max_drop_share` 0.4 → 0.5**, calibrated against the line-depth table (a conditional row in the calibration
  table). 0.6 overshoots with the passer free at 18–26 m (0.49–0.54).

**Tried and dropped.**
- **Feed-forward of the anchors' velocity** for positional play (block rates in the brain): the striker lag fell
  2.7 → 1.7 m and box mates rose 0.20 → 0.52. But off-ball speed rose 3.49 → 4.51 m/s (real ≈ 2–2.5), walking fell
  30 → 13 % and lab goals rose 2.4 → 4.7.
- **`position_full_speed_dist` 20 → 10:** lags 30–40 % shorter, speed 4.08 m/s.
- **`settle_radius` 2 → 1:** no effect.
- **The striker mirroring the defender who sets the offside line** (target velocity), with a 2–4× steeper catch-up:
  the most advanced attacker came only 1 m closer (striker lag 1.4–1.9 m), speed rose 3–6 % and goals 3.0 → 4.2.
- **Layer-3 noise off** (drift 0, score noise 0, `commit_cost` ×3): speed 3.49 → 3.42 m/s only. The running comes from
  anchors that move with every ball movement; the engine passes ≈ 40 % more often per minute in play than real teams.
- **A commit ratchet** (no switch cost for a point closer to the anchor than the kept one). It exposes a dead zone:
  `commit_cost` (0.3 per metre) outweighs the slope of the quadratic shape cost ((d / 6)², d / 18 per metre) within
  ≈ 5.4 m. A point up to ≈ 5 m off the anchor therefore has no pull back, and every role sits 3.3–5.7 m off its
  anchor. The ratchet trims that by 0.4–0.9 m and speed by 2 %, but leaves the striker's depth unchanged.

**Results** (`final_third` / `shot_share` 12 matches, `layers` 16, lab 40):
- Drop share, central, ball 18–30 m: 0.39–0.45 free and 0.35–0.37 pressed (real 0.39–0.43 / 0.33–0.38). Ball
  30–38 m: still 0.31–0.37 (real 0.39–0.45). The 8 and 16 m BLK-02 gaps bind there.
- The most advanced attacker is still 5–7 m behind the deepest defender; box mates 0.2–0.4 against 0.7–1.7 real.
- Shot share at 22–30 m 28 → 26.7 % (real 7.9 %).
- Lab: 2.95 goals from 3.03 xG (2.17 from 2.61). Shots 22.0 / 21.6 per team. Low block line 12.4 → 15.4 m.
- `movement`: 3.54 m/s, walking 29 %.
- Gap table (before → after):

  | | goals |
  |---|---|
  | 68 v 47 | 3.31–0.38 → 4.25–0.19 |
  | 58 v 47 | 2.12–0.62 → 2.56–0.62 |
  | 68 v 68 | 0.69–1.25 → 1.00–1.44 |
- `styles`: shots per team-match 14.3 → 15.9 (default); the other columns are unchanged within noise.

**Open.** The attackers' depth is a movement problem, not a target problem: anchor and target are close to the line,
the players are not. Fixing it in isolation raises the off-ball speed, which is already ≈ 40 % high. The next round
should take the movement model as a whole:
- the commit dead zone;
- target jitter from anchors following every pass;
- a finite-time approach in place of the first-order one.

Then measure the depth again.

## Pin the line or offer short (2026-10-30)

The final-third round exempted role ST from ATT-14, ATT-15 and the ATT-08 lane bonus short of his anchor, so strikers
always stayed on the last line. That was a blanket role switch and a misreading of the source:
- DFB-SCHOEN's "Nicht entgegenkommen!" is a hint for the central midfielder.
- Its striker correction is situational. The strikers stand in the cover shadow of two midfielders who have stepped
  up, so they should go deep, while a midfielder offers in the gap.

Coach-education material frames the choice as one made from cues, again and again, and varying with the player (catalog
ATT-26 has the quotes):
- the passer's state: an open ball means run, head down or pressed means drop to feet [FA-FWD];
- the partner: if one striker offers, the other holds the depth [DFB-RP16]; the 10 uses the room the striker's run
  opens [BDFL-HALE];
- the type of 9 [FIFA-N9-WC22; FIFA-U20-CF].

**What the real data shows** (StatsBomb 360, open-play passes with the ball 10–50 m from goal; g = a team-mate's
distance short of the deepest outfield opponent; scratchpad scripts `sb_front_pin.py` and `sb_fwd_recv.py`):

| line's distance from goal | nobody within 2 m of the line, passer free / pressed | second mate ≥ 5 m short when the front one pins |
|---|---|---|
| 0–12 m | 25 / 24 % | 10–13 % |
| 12–18 m | 25 / 26 % | 15–18 % |
| 18–25 m | 34 / 40 % | 26–29 % |
| 25–35 m | 43 / 49 % | 35–40 % |
| 35–60 m | 50 / 58 % | 41 % |
| all | 33 / 38 % | |

| forward receptions in the attacking half, on the line / short / dropped | all | line 0–15 m | 15–22 m | 22–30 m | 30–60 m | passer pressed |
|---|---|---|---|---|---|---|
| centre-forwards | 21 / 46 / 33 % | 42 / 42 / 15 | 19 / 49 / 32 | 13 / 42 / 45 | 9 / 53 / 38 | 20 / 52 / 28 |
| wingers | 13 / 37 / 50 % | | | | | |
| attacking midfielders | 9 / 29 / 62 % | | | | | |

Both the situation and the player matter:
- Lines close to goal are pinned more.
- The dropped share per centre-forward (≥ 60 receptions) spreads from p10 15 % through p50 32 % to p90 51 %. Lukaku
  is at 9 % and Benzema at 51 %.

**Model** (ATT-26, `positioning::offer_weights` and `score_attack`):
- Every front-line player, whatever his role, weighs pinning the line (or running in behind from it), checking short
  and dropping between the lines.
- The weights are a softmax over three things:
  - his profile from attributes (runner, hold-up, link);
  - the cues: a mate already on the line; an open ball or a mate offering short; a closed carrier;
  - the player instruction `come_short`.
- The lane bonus (ATT-08) is weighted by the offer at that depth. Within 8 m short of his anchor the shape no longer
  pulls him back to the line.
- The first version kept the lane bonus unweighted on the line and the shape pull intact. The line then won whatever
  the cues said: an all-rounder with a team-mate on the line still stood on it. The principle test
  `att26_front_player_offers_by_profile_and_by_who_pins_the_line` checks four cases in a fixed situation:
  - runner vs link profile;
  - alone vs with a winger on the line;
  - carrier open vs facing his own goal.

  Over 40 seeds the all-rounder splits about half and half between the line and 5–7 m short, a runner is always on
  the line, and a link player is always 5–7 m short.

**The `forward_moves` bench** mirrors both real scripts. It adds the following:
- pinning by target as well as by position;
- strikers' receptions by their label when the pass was played;
- strikers sampled every 0.2 s by time since the team won the ball;
- a per-player table.

`PROFILE=runner|target|link` changes the home striker's attributes. Harbour's W are 4-4-2 wide midfielders and not
comparable to real wingers.

| home striker (12 matches) | target on the line / short / dropped | position | receptions dropped |
|---|---|---|---|
| runner | 60 / 36 / 4 % | 16 / 50 / 34 % | 48 % |
| target man | 27 / 57 / 16 % | 7 / 51 / 42 % | 54 % |
| link | 19 / 54 / 28 % | 4 / 46 / 50 % | 62 % |

Nobody pins at 39 / 51 % of passes by target (real, by position, 33 / 38 %). The profile effect now reaches positions
and receptions.

**The lag.** Positions trail targets badly. Strikers sampled in possession by time since the regain:

| since the regain | target g | position g | position dropped while the target is not |
|---|---|---|---|
| 0–3 s | 5.9 | 14.3 | 59 % |
| 3–6 s | 3.1 | 9.5 | 50 % |
| 6–10 s | 3.0 | 7.2 | 31 % |
| 10–20 s | 3.0 | 6.0 | 22 % |
| 20+ s | 3.1 | 5.8 | 22 % |

The lag is steady-state, not only after transitions. 73–76 % of strikers' receptions start with a "receive" label;
the striker is then 5.5–6.4 m short of the line and controls the ball 8.4–8.9 m short of it, because he meets the
ball (ATT-24). Receptions on the line are therefore 2 % against 21 % real.

**Regression** (lab 40, `layers` 16, `styles`, `shot_share` 12; before → after):
- Lab: 2.95 goals from 3.03 xG → 2.65 from 2.81; shots per team 22.0 / 21.6 → 21.2 / 19.5; completion 80–81 %.
- Gap table:

  | | goals |
  |---|---|
  | 68 v 47 | 4.25–0.19 → 3.88–0.38 |
  | 58 v 47 | 2.56–0.62 → 2.12–0.31 |
  | 68 v 68 | 1.00–1.44 → 1.00–1.25 |
- `styles`: unchanged within noise; shots per team-match slightly lower (default 15.9 → 14.8, possession 23.7 → 20.5).
- Shot share at 22–30 m: 26.7 → 30.0 % (≈ 1.5 standard errors; real 7.9 %).

**Open.**
- The movement model (the previous round's open item):
  - the commit dead zone;
  - targets that follow every pass;
  - the first-order approach;
  - the in-possession block front, which follows the offside line at `def05_shift_speed` (3 m/s).
- The line gradient is inverted. Real teams pin more when the line is deep (in the box). The engine pins less there:
  by target, nobody pins at 44–45 % with the line 0–12 m from goal and 36–48 % at 25–35 m. Box occupation (FT-03,
  M4) is the likely cause, not a sourced line-depth cue. FIFA-GI-BIB even advises coming short against a deep line.
- No runs in behind at all (`forward_moves`, 12 matches): attackers start 0.0 runs per player-match (FIFA: Number 9s
  offer in behind 26.5 times per 90 min), and receive beyond the line 1.0 times per team-match (StatsBomb 360: 7.6 at
  engine time). The ATT-18 run point is scored like any other point and never beats the shape cost. ATT-26's
  "pin" therefore means standing on the line, never running from it.

## Player brain redesign (proposal, 2026-10-30)

The fixes since the final-third round kept adding score terms to off-ball positioning without fixing the causes:
- players choose points, not movements;
- coaching rules are additive terms whose weights are the behaviour;
- off-ball players value positions in a different currency from the carrier;
- coordination is central out of possession (the team brain) and missing in possession.

[player-brain.md](player-brain.md) proposes the following:
- **Individual brains.** Every player has his own brain, in and out of possession; there is no team brain.
- **Roles from the formation.** Each slot's role sets the player's duties, reference positions and preferred
  movements, and the instructions modify them.
- **Movements instead of points.** Pin, check, drop, run in behind, show, overlap, decoy and the rest each have a
  trigger, a path and a timing.
- **One currency.** A movement is valued by the pass option it creates, using the carrier's own pricing.
- **Look-ahead by skill.** Instinct and intelligence set how far ahead a player looks.
- **Coordination by reading.** It works through a shared concept and through reading team-mates. Reading quality
  comes from intelligence, how much players have played together and how practised the system is, not from noise.

Status: the review decisions are in; phases 0 (the attribute palette) and A (per-player brains) are done, B–G are not
started.

## Attribute palette, phase 0 (2026-10-31)

The 40 ratings became the 28 of [attributes.md](attributes.md): body, technique, mind, character and goalkeeping,
plus height, weight and foot. Where each one acts is listed there (section 8). The changes in behaviour:
- **Kicks use their own skill.** Ground passes use short passing up to 20 m and long passing from 35 m, blended in
  between. Lofted passes and clearances use long passing, and crosses (`pitch::is_cross`: from a wide channel of the
  final third into the box) use crossing. Shots use finishing up to 12 m and long shots from 22 m, blended in
  between (it was a hard switch at 18 m). The general technique rating is gone, so the shank chance now falls with
  the kick's own skill.
- **Weight counts in challenges only.** Challenge strength = strength + 1 point per kg above 75 kg
  (`duel.weight_strength_per_kg`, `weight_ref_kg`). It feeds tackles, take-ons, aerial duels and shielding, not
  locomotion.
- **Keepers.** The weight of the save skill moves from reflexes to one-on-ones as the shot gets closer, between
  16.5 m and 8 m (`save_one_on_one_far` / `_near`). The smother uses one-on-ones, and the dive reach uses quickness
  and reflexes.
- **One egotism axis.** Teamwork is folded into egotism (team player 0 … egotist 100), and
  `personality.teamwork_damping` is gone. Bravery is folded into aggression (`aggression_risk_reduction`).
- **Minds.**
  - Intelligence sets reliability and the vision cone.
  - Instinct sets reaction, touches on passing balls, blocks and the choice temperature.

The benches with uniform ratings did not move. The squad conversion first raised shots by 12 %, because the merged
instinct sharpened the choices of squads written with high anticipation. Instinct is therefore converted as 0.25
anticipation + 0.75 decisions, after an ablation. Lab: goals 2.75 (was 2.65), shots 22.4 / 20.1 (21.2 / 19.5),
interceptions −3 to −4 %, the rest unchanged.

## Player brains, phase A (2026-10-31)

The team brain is gone as a single decision maker. Every player keeps `Beliefs` (`ai::brain`): phase, blocks, every
team-mate's duty, pressing, the loose-ball claim. Each player evaluates the former team-brain logic himself, from
his previous beliefs and the world cache (`ai::world::World`). The cache holds the physics and geometry brains ask
for, each query evaluated once per update: offside lines, units, pressure on the carrier, times to points, intercepts,
receptions, lane openness. Positioning, anchors and the on-ball AI read the player's own beliefs. Stats, the viewer,
the benches and the tests read `MatchState::team_view`.

Perception is exact, so all players of a side reach the same beliefs (`tests/brains.rs`), and the matches are
unchanged: all 40 lab event-log hashes are identical to phase 0. 22 brains cost +0.17 s per match (6.6 → 6.8 s
single-threaded).

## Roles, phase B (2026-10-31)

A formation slot names two roles, one in possession and one out of possession (`Slot::role`), from the 14 roles of
the catalog (ROLE-01 … ROLE-14, `team::role`, defined in `data/roles/*.ron`): unit, lane, duties, movement priors,
relations and accepted instructions. Formations shipped: 4-4-2, 4-3-3, 4-2-3-1, 3-4-2-1, 3-4-3 and 3-5-2
(`data/tactics/`). The brains read duties instead of role names (the last line, the rest defence, runs in behind),
and each player's role familiarity replaces `attunement`. Today's 4-4-2 and 4-3-3 were mapped onto the new roles
(4-3-3: CM8, W in possession / WM out of it, CF; 4-4-2: CMP, WM, ST2) and play identically: all 40 lab hashes are
unchanged.

## Defence with individual brains, phase C (2026-10-31)

`AiConfig::brain_model` switched between `Legacy` (the phase-B behaviour, kept so every step could be compared; its 40
lab hashes equalled phase B's) and `Individual` (the default) until phase G removed it. With individual brains, out of
possession:

- **Movement executor** (`ai/movement.rs`, `Player::plan`). An outfield player chooses his movement (press, cover,
  mark, track, recover, hold the block) at a trigger and holds it. A trigger is any change of the situation he chose it
  in: possession, his duty, the press, the team phase, a pass struck, a new carrier. Without one he re-checks the choice
  every `movement_check_s` (0.75–1.25 × by layer-3 reliability). In between he follows the movement, re-reading its
  references (carrier, man, block) at his reaction time instead of the off-ball decision interval: a held block point
  moves with his anchor without re-scoring the candidates. Arrive-at-time for the press is the chaser's brake logic
  (PRS-02), already present.
- **Line leader** (BLK-02, `Beliefs::line_leader`, `line_step`): the line-leading role nearest the block's centre owns
  the call to step or drop and changes it only after the reading has held for `blk02_call_hold_s` (at once on a back
  pass).
- **Marking** (DEF-04, IND-04): free opponents are claimed most dangerous first (denial value = possession value where
  he stands × lane openness from the ball); a marker moves with his man (target velocity); a man running goal-ward out
  of his marker's zone is handed over to a team-mate goal-side of him or tracked (`DefDuty::Track`,
  `def04_track_speed`, per team `DefendingInstructions::track_speed`, `def04_handover_distance`); a man coming short is
  let go.

Lab 40, legacy → individual: goals 2.75 → 2.55, shots 22.4 / 20.1 → 20.7 / 18.3, passes, tackles and the block
metrics unchanged within noise, off-ball mean speed 3.48 → 3.50 m/s, positioning kind changes 25.8 → 26.3 per
player-minute. The movement executor's own effect (fewer re-scorings) is small because most defensive targets were
already re-read every decision; its point is the structure the later phases build on (offers and runs in phase D).

## Front-line movements, phase D (2026-10-31)

With individual brains, a front-line player in possession (ATT-26: not rest defence, anchor near the front of the
shape) chooses a movement from his role's repertoire and holds it: **hold**, **pin** the line, **check** to feet,
**drop** between the lines, **run in behind** (`positioning::front_movement`, docs/player-brain.md sections 6 and 8).

- **Offers** (`ai/offer.rs`). A movement is worth the option it gives the carrier: the ball to its best point, priced
  as `on_ball::passes` prices a pass (possession value, line breaks, forward terms, pressed-receiver loss, interception
  cost) with a cheap race along the lane and to the point. The carrier plays only what he sees after turning to it; a
  ball in behind only if he is open, as well as his passing allows. Whom the offer is for (`offer::passer`): the
  carrier, or the receiver of a team pass in flight once it is due within the player's look-ahead (`lookahead_horizon_s`
  × mean of instinct and intelligence), who takes it facing the ball.
- **Value** (`positioning::front_scores`, `FrontScore`): marginal value (what the offer adds to the carrier's expected
  best option, a softmax inclusive value over his team-mates' offers where their movements offer, with uncertain balls
  in behind enumerated), plus the space value of the response of the man marking him (weighted by look-ahead), less
  effort; role prior and instructions (`forward_runs`, `come_short`, times adherence) as shares of the local value
  scale; noise and a switch margin from layer 3.
- **Execution**: a pin moves with the line at its speed; a check bursts; a run waits onside in its lane and sprints
  when the carrier sets the pass up (ATT-24), and the through ball aims at the run's offer. He re-chooses at a trigger
  (now also the carrier turning open or closed), the periodic check, the end of a check or drop, a line shift beyond
  4 m, or a pass due within his look-ahead.

Lab 40 (individual): goals 2.55 → 4.0 per match (real 2.7 × 1.48 ≈ 4.0 at engine time), xG 1.48 / 1.16 → 2.09 /
1.70 (real ≈ 1.9 per team), shots 20.7 / 18.3 → 26.5 / 23.9 (too many: xG per shot 0.075 vs ≈ 0.10 real, the open
shot-selection issue), pass completion 81–82 → 79–80 %, interceptions + 12 %, mid-block length 28.9 → 31.3 m. Legacy
reproduces phase C's 40 hashes. `forward_moves`: strikers' targets are on the line 55–75 % of the time with the passer
free, but their positions lag (on the line 11–17 %; g 14.6 m in the first 3 s of a possession, 4.6 m after 20 s), so
"nobody within 2 m of the line" is 72 % at the pass vs 33 % real, and strikers' receptions are dropped in 56 % (33 %
real). Off-ball speed 3.50 → 3.73 m/s (`movement`). Open: the lag of front players behind their targets after a
transition, shot selection, and off-ball speed (fatigue, M6).

## Possession movements, phase E (2026-10-31)

Every outfield player in possession now chooses a movement and holds it (`positioning::possession_movement`,
docs/player-brain.md sections 6 and 8), from his role's repertoire: hold, **show** (a ring round the carrier at
10 m, 16 m and his support distance, reached within `show_reach_s`), **in to out / out to in**, **overlap /
underlap** round a team-mate on the ball out wide, **invert** into midfield, **drop into the back line** between two
centre-backs, **step into midfield**, plus phase D's front-line movements. Rest defenders have their own repertoire
(hold, show, invert, drop into the back line, step into midfield), never ahead of `tr05_gap` behind the ball.

- **One value for all** (`score_moves`, `MoveScore`): the offer's marginal value to the carrier's expected best option
  (team-mates' offers included, `offer_uncertainty` widening the temperature because the options change before he
  decides), plus the space its marker's response leaves, less effort, plus prior and instructions. Team-mates'
  movements are read one level deep (`offer::responded`: their markers follow them; a zonal marker lets a man coming
  short or across out of his zone go, DEF-04). Keeping the ball is priced with the first defender's approach over
  the look-ahead (`offer::keep_value`), so an offer is worth more to a carrier about to be closed.
- **ATT-10 without a penalty term**: an offer screened by a team-mate within 1.5 m of the lane cannot be played; two
  offers within `offer_overlap_m` are one option; following a movement takes the candidate nearest the last offer
  that a team-mate's plan does not already cover.
- **Rest defence by situation** (`brain::assign_attack_duties`, TR-05): claims go deepest first, but a wide player
  on the ball's side claims later (`tr05_ball_side_release`) and `forward_runs` / `overlap` move the claim
  (`tr05_instruction_shift` × adherence). New player instructions `overlap` and `invert` (viewer sliders).
- **Noise kept per decision**: the layer-3 choice noise is drawn at a trigger and kept until the next
  (`Plan::noise`), so a held movement no longer re-rolls its points each check.

Lab 40 (individual, D → E): goals 3.58 → 2.83, shots 26.5 / 23.9 → 24.6 / 21.3, xG 2.09 / 1.70 → 1.86 / 1.39, pass
completion 79–80 → 80 %, one-touch goals 9 → 14 %, goals from 1–5 passes 29 → 23 % (real 53 %). In-possession shape:
build-up 33.7 × 41.1 → 33.2 × 38.5 m (real 39.2 × 54), progression 32.3 × 48.3 → 30.9 × 45.3 m with the line 33.3 →
34.4 m (real 33 × 55, 40). Legacy reproduces phase D's 40 hashes. `movement`: 3.73 → 3.79 m/s, walking 25.9 → 24.9 %
(test floor lowered to 22 % with history). Test 13: a full-back averages x −9.3 and stays back 54 % of the time, a
wing-back x −1.2 and 11 %; both reach the touchline in possession, so their width is equal.

Open: the in-possession width is too narrow (full-backs show and invert often and the wide points are worth little
until crosses and cut-backs exist, M4; overlaps are only chosen with the instruction); positions lag targets after
transitions; off-ball speed (M6). Test 6 (the 9 drops, the winger runs in behind) moved to M5 with BLK-03: the space
opens only when the back line shuffles across as a centre-back follows the 9.

## Reading team-mates, phase F (2026-10-31)

Players now hold their own beliefs about their team-mates (docs/player-brain.md section 7; `ai/reading.rs`). How well
player i understands team-mate j is u_ij = intelligence × F_ij, with the familiarity F_ij = f(practice) × f(partners)
× f(1 − instruction load) and f(x) = floor + (1 − floor)·x. New team data: `Practice { formation, concept }` and
`Partners { default, pairs }` per squad (default 0.9 each), and `Settledness` presets for the familiarity bench. The
instruction load is the larger of (1 − role familiarity) and (1 − concept practice) × the player instructions summed.

- **Duty claims** (`brain::assign_duties`, `read_team_mates`): each player resolves the out-of-possession roles
  (DEF-01…04, IND-06) and the TR-05 rest-defence claims from his own estimates. He knows a team-mate's times as far as
  he knows him, judging one he does not know as a player of average pace (`reading::estimated`). After every player's
  own update a second pass reads team-mates' claims: a claim that differs from what i's concept prescribes, now and at
  his last update, and has been held for d_ij = `reading_delay_max_s` × (1 − u_ij), is held fixed while i resolves
  his own duty again. Of two players claiming the same first-defender role, mark or track, the earlier claim keeps it.
  Observers (`MatchState::team_view`) see the claims (`brain::view`).
- **Movements** (`reading::seen`): every possession movement carries the concept's choice (`Plan::concept`, a perfect
  reader's choice without noise) when it differs. Until i has read the movement, he sees the concept's: in ATT-10
  screening, in the offers of team-mates his own choice competes with, in their markers' responses, and for a through
  ball into a run.
- **Instructions** apply as far as the team has practised the concept (`Player::instruction_familiarity` = role
  familiarity × concept practice). A team-mate's instructions count in the TR-05 claim order as far as practised.
- **Reading the passer** (`reading::receive_delay`): the intended receiver starts to take a pass
  `reading_receive_delay_max_s` × (1 − F) after it becomes legible (the set-up or the strike). Until then he keeps
  his movement, and the passer prices the delay in the pass's race. Intelligence is left out here because the
  receiver's general reading is already in his reaction.
- **Not read in F:** the loose-ball chaser and the IND-07 challenge (wrong estimates there need a correction, the
  "mine" call, M6), and the BLK-02 line call (its cue is exact; line errors come with M6 perception).

Why the receiver. Claims and movements alone left the familiarity bench at zero. "Just assembled" scored xG +0.005 ±
0.16, and splitting the level into role, partners and practice gave nothing outside noise. Errors among claims last
≈ 0.3 s at the crossings of two team-mates' times, and the passer priced every pass with the true race, so an unread
movement cost an option, never a ball. The research places the effect in pair passing instead: completion and
frequency, goals scored rather than conceded ([familiarity-research.md](tactics/familiarity-research.md) section 5).
With the receiver's anticipation, passing degrades in order of the levels.

Calibrated values: `reading_delay_max_s` 1.5 s, floors practice 0.7, partners 0.2, load 0.3, and
`reading_receive_delay_max_s` 0.6 s. A settled pair's receiver is ≈ 0.1 s late; a just-assembled one is ≈ 0.5 s late.
Familiarity bench (`examples/familiarity.rs`, one squad against itself, 160 matches per level, settled side's edge,
targets from the research in goals):

| level (role / partners / practice) | xG | goals | target | settled vs level completion | passes |
|---|---|---|---|---|---|
| new system (0.9 / 0.9 / 0.2) | +0.065 ± 0.057 | −0.13 ± 0.12 | +0.04 (0 to +0.10) | 79.4 vs 79.0 % | 1022 vs 1008 |
| signings (0.8 / 0.5 / 0.6) | +0.013 ± 0.065 | −0.04 ± 0.14 | +0.08 (+0.02 to +0.17) | 79.0 vs 77.9 % | 1018 vs 976 |
| just assembled (0.6 / 0.1 / 0.2) | +0.313 ± 0.064 | +0.12 ± 0.14 | +0.25 (+0.10 to +0.50) | 78.4 vs 75.4 % | 1013 vs 914 |

xG is the measure: goals are twice as noisy. Each level is within one standard error of its target, and resolving
+0.04 against +0.08 would take thousands of matches. Coordination errors among the claims (nobody or two pressing, a
man marked twice) rise from 0.13 error-seconds per live minute for a settled side to 0.32 for an assembled one; their
episodes last ≈ 0.3 s.

Lab 40 (individual, E → F): goals 2.83 → 3.25, shots 24.6 / 21.3 → 25.2 / 22.7, xG 1.86 / 1.39 → 1.80 / 1.60, passes
1003 / 1094 → 982 / 1065 (79–80 %), goals from 1–5 passes 23 → 39 % (real 53 %), one-touch goals 14 → 13 %; shapes
within noise. The squads' default practice and partners (0.9) apply instructions at 0.9 strength, in the legacy model
too, so Legacy no longer reproduces phase E. With practice and partners 1 and no reading delay the individual brains
still give phase E's 40 hashes. Tests: 11 and 12 (`tests/brains.rs`); the perfect-team tests (one reading, test 10)
and test 13 use squads that know each other.

## Clean-up, phase G (2026-10-31)

The team brain's legacy decisions are gone, and with them `AiConfig::brain_model`. Removed: the legacy in-possession
scorer (`positioning::attack`, `score_attack`, the ATT-26 `offer_weights` and cues), nearest-free-defender marking,
the fixed line step, the TR-05 claim order without situation or instructions, the ATT-09 option count, and the 13
parameters only they read (`att08_lane_weight`, `att09_boost`, `att10_collinear_penalty`, `att10_same_line_tol`,
`att10_crowding_radius`, `att12_channel_bonus`, `att13_central_bonus`, `att14_penalty`, `att15_between_bonus`,
`att26_pin_bonus`, `att26_pin_band_m`, `att26_profile_slope`, `att26_cue_shift`). `att09_min_options`,
`att10_collinear_deg` and `att14_shadow_deg` stay as the thresholds of the tests that check those principles emerge.
The catalog's translations describe only the movement model now, with a history line where a term went.

The 40 lab matches are bit-identical to phase F, so the switch hid no behaviour of the individual brains.

## Wide play, M4 (2026-10-31)

Crosses, box runs, defending crosses, keeper claims, the touchline trap and the outnumbered defender (catalog FT-01…03,
CRS-01, GK-04, PRS-05, IND-05).

- **Crosses (FT-02).** A pass from the wide final third into the box is a cross (`is_cross`). `offer.rs` prices it
  as a ball over the lane: no lane interception, no ATT-10 screening. Opponents who reach the target in time contest it
  in the air (keep share from `aerial_win_prob`). The keeper races only the flight time at 17 m/s, the real median
  horizontal speed (GK-04: he comes once it is played). Availability is 0.5 + 0.5 × crossing. On the ball a cross to
  feet is lofted at `cross_launch_angle` 0.33 rad, fitted to real hang times (0.87 / 1.23 / 1.62 s at 0–15 / 15–25 /
  25–35 m), instead of the 0.65 rad of long passes.
- **Box runs (FT-03).** `Movement::BoxRun` joins the in-possession repertoire (14 movements). Its offer points are the
  four spots `ft03_spots` (near post, far post, penalty spot, edge), offered while the carrier is in the attacking
  third outside the width of the box. `score_moves` now takes a movement's point by its marginal value rather than its
  own offer value, so two runners take different spots. The runner holds onside until a team-mate's cross is in the
  air, then runs onto the spot. A carrier who has read the run plays a lofted cross or a ground ball to it.
- **Defending crosses (CRS-01).** With the ball wide in the own defensive third, the line drops to no more than
  `crs01_line_depth` 14 m from the goal line, whatever the pressure (`DefendingTactics::cross_line_depth`, instruction
  `cross_line_depth`).
- **Keeper claims (GK-04).** The keeper is his team's chaser for an opponent's pass into his area only if he arrives
  `gk04_claim_margin_s` × (1 − aerial command) before the receiver. Otherwise PRS-02 sends the best outfield
  interceptor.
- **Touchline trap (PRS-05).** In a press at the touchline, the covering defender closes the side the first defender
  leaves open, `prs05_trap_distance` from the carrier (down the line when the team shows wide). The share `prs05_trap`
  is instructable (`trap`).
- **Outnumbered (IND-05).** A first defender with no team-mate near the carrier and an unmarked attacker near him
  delays on the bisector between the goal and that attacker, cutting the pass.
- **Scenario fix.** `tests/common::settled` now places the keepers where `positioning::decide` sends them, not on
  their formation anchor (the away keeper had stood at x = 20 m). Two tests changed with it, with history notes:
  ATT-14 checks the best-marginal offer, and the practice test uses seeds 5–8.

Wide bench (`examples/wide.rs`, 40 matches; real values from StatsBomb 360 open data, 166 matches, per team-match ×
1.48):

| measure | engine | real |
|---|---|---|
| crosses per team-match | 11.5 | 11.4 |
| completed | 30.5 % | 30.4 % |
| followed by a shot | 13.2 % | 16.0 % |
| keeper first (lofted / ground) | 6.6 % (4.0 / 10.3) | 2.3 % (3.1 / 0.9) |
| by band x 80–96 / 96–108 / 108–120 | 7.6 / 2.9 / 1.0 | 2.2 / 3.8 / 5.4 |
| team-mates / defenders in the box at the release | 0.59 / 2.0 | 2.35 / 4.54 |
| team-mates in the box at the first contact | 1.0 | 3.3 at arrival (FIFA) |
| cut-backs per team-match | 0.25 | 0.92 |

Outcomes per band match the real ones, but the mix does not: crosses come too early. In deep wide positions the
alternatives are undervalued (`value_wide` 0.4 in the possession value), and the box is under-occupied at the release
because both lines are still moving (defenders 5–9 m short of the CRS-01 line target, runners onside behind them).
Both are left open for the report rather than recalibrating the value field in M4. Lab 40: goals 3.08–3.38 across the
M4 steps (noise ± 0.3), shots 24.6 / 23.5, pressers 52 / 43 / 6 % (target 2–3 ≈ 58 %, 4+ ≈ 25 %).

## Instructions and the remaining tactics, M5 (2026-10-31)

- **Stepping out (BLK-03).** A back-line defender follows a man who drops between the lines (more than
  `blk03_front_m` in front of the line) only up to his `step_out`, only while no other line defender is out, and
  only if a line team-mate whose slot is within `blk03_shift_m` can shift across. Otherwise he passes the man on to
  the midfield. Whether he goes is decided as the man drops; one already out keeps following. While he is out, the
  back-line players on the side away from the ball shift into his slot by `shuffle` of the gap. Before M5 a
  centre-back followed anyone in his 12 m DEF-04 zone. Test 6 (`blk03_…`, `tests/principles.rs`) shows that with the
  step-out and the shuffle, the far winger's run in behind is worth more, as an offer and to the carrier, than when
  the line holds.
- **Overloads, first pass, running lanes (ATT-19, TR-02, TR-03)** have no rules of their own. Each comes out of the
  marginal value of offers and the pricing of passes. TR-03 is checked in test 10: with perfect readers, duplicate
  runs occur in under 1 % of counter-attack ticks.
- **Manager options (`team::options`).** `TeamOption` (20 options in 10 pairs) sets the engine-unit instructions as
  multiples of, or steps from, their layer-1 catalog values. `PlayerPreset` (false 9, target man, runner, inverted
  and overlapping full-back, stay back) is a named combination of the player instructions. The catalog's
  instruction lines name the options.
- **Test 12** now checks the reading delays directly: practice 1.0 vs 0.2 gives 1.03 vs 1.15 s for team-mates and
  0.28 vs 0.36 s for passes. Over twelve seeds the full-match error count no longer separated the two levels (33.8
  vs 33.1 error ticks per 1000).
- **Not in M5:** a shape editor (a manager UI, not the simulation).

Lab 40: goals 3.47, shots 23.7 / 21.8, shapes unchanged.

## Full layer 3, M6 (2026-10-31 – 2026-11-01)

Fatigue, pacing, event-driven confidence, concentration lapses, calls and scanning. The parameters and their sources
are documented in `config.rs`; the catalog entries are ATT-05 (scanning) and LAW-04 (substitutions use the reserve).

- **Fatigue (`player::fatigue`).** Two stores. Running above `fatigue_hi_speed` (19.8 km/h, the tracking studies'
  high-intensity threshold) drains the short-term energy, which recovers below it towards the long-term reserve. The
  reserve falls with the work of every metre run (∝ speed²). Stamina scales both drains. Energy takes top speed and
  acceleration down, makes optional efforts dearer (`effort_fatigue`, section 8.5 of docs/player-brain.md), lengthens
  the pace times and raises the lapse rate.
- **Pacing (`positioning::pace_time`).** Positional play, cover and marking close the gap to the target within a pace
  time at no more than `pace_run_speed` (below the high-intensity threshold), instead of at the speed the gap allows.
  Pressing, tracking a runner, receiving, chasing and the efforts of the possession movements are not paced; a
  recovery run goes as fast as it needs to be goal-side in time. Without pacing a player ran 19.4 km with 8.2 km at
  high intensity per match (goals 4.15); pacing took that to 13.5 / 2.4 km.
- **Confidence (`player::confidence`).** Every event steps the player concerned (completed or lost pass, duel won or
  lost, miscontrol, shot on target, goal, save), damped by composure. Conceding lowers the whole team, scoring gives
  the team nothing [HORRILLO23; HEUER10; GAURIOT18]. Confidence returns to the pre-match level in ≈ 5 minutes, faster
  with a leader near him.
- **Lapses (`deviation::lapse_rate`).** At an off-ball re-read a player may switch off for `lapse_s`, more often with
  low composure and low energy [SMITH16; BADIN16].
- **Calls (`reading::call_rate`, docs/player-brain.md section 7).** Calls per minute by leadership, confidence and
  load, a few in hand. A claim, staying back, a change of the line call or "mine" reaches team-mates at their next
  update instead of after their reading delay; "man on" refreshes a receiver's picture of the opponents.
- **Scanning (`ai::scan`, ATT-05).** Opponents outside a player's field of view are where his last scan saw them.
  Pressure, the controlling touch and the passer's offside judgement use that picture; a fresh scan settles him faster.
- **Lab lines** (new): running per player-minute by period (starters only), goals and shots by 15 minutes, the goal
  rate and completion after a goal, scans and the completion by scans, lapses and calls.

Ablation (40 matches each, before M7's added time): with all four fatigue effects off (speed loss, effort, pacing,
lapses) the second half ran 1 % more than the first, high-intensity running in the last 15 minutes was 5 % above the
first 15, and goals rose from 2.10 to 2.95; each effect alone costs 0.2–0.4 goals. The share of shots in the last 15
minutes fell even with every effect off (14 % against 17 % in the first 15), so the late-match decline in shots is not
fatigue (open, for M8).

Bench (round h of M7, 40 matches, per 90 minutes): 14.0 km per outfield player, 2.88 km at high intensity (real 10.6–11.4
and 0.53–0.98 km [DISALVO07; JU23], ≈ 17 / 1.2 km scaled to the time in play: high-intensity running ≈ 2× too much,
open); second half −13 % (real ≈ −7 % [FIFA-WC22-RUN]); high-intensity running in the last 15 minutes −38 % against the
first 15 (real −20 to −45 % [BRADLEY09; MOHR03]). Scanning 0.44 scans/s (real 0.44 [JORDET20]), but completion by scans
0–2 / 3–4 / 5+ is 82 / 85 / 88 % against 64 / 68 / 81 % [JORDET13]. After a goal, the scoring team's goal rate is ×1.28
and ×0.71 of its own (real ≈ ×1; ×0.84 / ×0.60 in round g, so mostly noise), and the conceding team's completion is
2 points above its own (real a little below): game state outweighs the confidence step (open). 74 lapses per outfield
player and match; calls per team: claims ≈ 2200, line 220, "mine" 260, "man on" 290 (no published rate).

## Laws and set pieces, M7 (2026-11-01)

Offside, fouls and cards with advantage, added time, substitutions, set-piece positions and the tactical foul
(catalog LAW-01…04, SP-01…05, TRN-04; sources in docs/tactics/sources.md, "Laws of the Game, set pieces and match
events").

- **Referee (`rules::referee`, `sim::laws`).** Offside marks are set when a player plays the ball and checked when a
  marked player becomes involved. A challenge is a foul with a probability by its outcome, from behind, and by the
  tackler's tackling and aggression, and is reckless (a caution) or uses excessive force (a sending-off) with set
  chances. SPA and DOGSO are judged from the circumstances Law 12 lists. Advantage is played in the fouled team's
  attacking half when the carrier stayed on his feet, and brought back if the ball is lost within 3 s. A sent-off
  player leaves; his team plays on with ten in the same shape.
- **Clock (LAW-03).** Each half adds the time lost through goals, substitutions, cards, penalties and a base
  allowance; the minimum is shown at the end of regular time and the engine plays it. With added time the ball is in
  play ≈ 92 of 90 minutes, so real per-match counts are now scaled × 1.62 (was × 1.48).
- **Substitutions (LAW-04).** A default policy until a manager makes them: from minute 45, players whose long-term
  energy reserve (M6) has fallen below a threshold that rises with time are replaced by the best-suited bench player,
  within five substitutes and three windows.
- **Set pieces (`ai::set_piece`).** While a corner or free kick is prepared both teams take set positions. Defending a
  corner: zonal players, an edge player, markers and outlets. Attacking a corner: tiered spots, a second-ball player
  and a short option. A free kick near goal gets a wall sized by distance and angle, the end man outside the near
  post; a crossed free kick gets a line. At a penalty the kicker picks a side, the keeper guesses, and everyone else
  stands at the edge of the area. At a goal kick the opponents keep out of the area. Once the ball is in play, open
  play takes over. The set-piece parameters are per team (`SetPieceTactics`, `SetPieceInstructions`).
- **Tactical foul (TRN-04).** A player beaten in the first 5 s after his team lost the ball, in the last 60 m towards
  the opponents' goal, may foul instead (`DefendingTactics::tactical_foul`).
- **Blocks and deflections.** An opponent's kick that reaches an outfield player within his reaction time of being
  struck, or any shot, is blocked rather than played: square on the body with `shot_body_block_p`, on a leg with
  `shot_leg_block_p`. A body block rebounds at 15–35 % of the ball's speed, a leg block is turned 0.25–1.2 rad and keeps
  40–75 % (`EventKind::Block`). A failed touch on a fast opponents' ball is a deflection that keeps 30–70 % of its speed
  instead of a heavy touch. A blocked kick is not a deliberate play (Law 11). A keeper's parry goes behind for a
  corner with `parry_behind`.
- **Options.** Eight more `TeamOption`s: stop counters with fouls / no tactical fouls, zonal / man-to-man at corners,
  leave players up / everyone back, commit to the box / hold back at set pieces. That makes 28 options in 14 pairs.
- **Lab and viewer.** The lab prints fouls, cards, offsides, penalties, free kicks, corners, goal kicks, substitutions
  and added time against the scaled StatsBomb counts. The running lines are now per player-minute and leave out the
  substitutes, like the tracking studies' full-match players. Before, the substitutes' fresh legs and the added time
  in each half's last period hid the decline: −13 % in the second half now, +1 % before. The viewer shows the added
  time on the clock and the cards.

Bench (round h, `match_lab` 40 matches, per team-match; real StatsBomb open data × 1.62):

| measure | engine | real |
|---|---|---|
| fouls (own / middle / attacking third) | 19.4 (26 / 65 / 9 %) | 19.6 (17 / 53 / 30 %) |
| tactical fouls | 2.9 | ≈ 2.7–4.8 [DATA-TF01] |
| advantages | 4.3 | 2.7 |
| yellow / red cards | 3.35 / 0.14 | 3.43 / 0.065 |
| offsides | 1.25 | 2.41 |
| penalties (scored) | 0.16 (77 %) | 0.18 (78–80 %) |
| direct free-kick shots (xG each) | 0.80 (0.068) | 0.66 (0.04–0.06) |
| corners (shot / goal within 10 s) | 1.95 (0.55 / 0.075) | 8.03 (2.60 / 0.178) |
| goal kicks (short) | 6.9 (5.8) | 12.3 (6.2) |
| throw-ins | 9.3 | 25.8 |
| blocks | 35 | 32 |
| clearances | 4.7 | 29 |
| substitutions (minute p10 / median / p90) | 4.56 (57 / 69 / 81) | 4.39 (51 / 72 / 87), unscaled |
| added time shown (1st / 2nd half), played per match | 3.9 / 7.8, 12.4 min | PL 2023/24 11:39 per match |

Goals 3.00, shots 20.1 / 19.1. Cards, fouls, penalties, tactical fouls, blocks, substitutions and added time are close.
The restarts are not. In StatsBomb's 40 matches the event before a corner is a clearance in 28 % of cases, a block in
22 %, a keeper facing a shot in 18 % and a save in 10 %. Throw-ins most often follow a clearance (16 %) or a block (15 %).
Real defenders clear 18.1 times per team-match (13.7 in their own box, 55 % headed). Engine defenders play their way
out of pressure, and their headed clearances are aimed upfield, so balls seldom go out: two thirds of the engine's
corners follow a save or a block. Fouls high up the pitch are scarce too (9 % against 30 %), and so are offsides. All
three are left open for the report, with this diagnosis, rather than modelled further in M7.

## Calibration lab, M8 (2026-11-01)

The plan's last milestone: realistic stat targets, tactical A/B experiments, a value field learned through self-play.
Nothing in it changes the simulation by default: a 40-match lab gives round h's hashes.

**Scorecard.** After the report the lab checks the calibration table of docs/tactics/principles.md row by row (range
from the source; ± 10 % where it gives a mean only). Situation-conditional rows (shapes in a phase, shares, rates per
event) are the targets; per-match volumes (real counts × the in-play scale) are references, since they depend on the
teams' styles. Default squads, 300 matches: 7 of 32 targets and 6 of 10 references in range. In range: mid-block width,
keeper to back line, counter-press duration, scans before receiving, high-intensity decline, substitution minute,
direct free-kick xG. Out: the block and in-possession shapes (lines 3–9 m too deep, only the build-up line 1 m too
high; width 40–45 m against 54–55 in build-up and progression), keeper 11.3 m from goal (13.1), presses by 2–3 players 40 % (58), one-touch goals 6 % (63–80),
goals from 1–5 passes 30 % (53), counter-attacks 33 m (70), second-half running −13 % (−7), fouls high up 8 %
(30), short goal kicks 84 % (51), restart volumes (corners, goal kicks, throw-ins ≈ ⅓ of real).

The 300 matches also put a number on the noise of the 40-match benches: seeds 1–300 average 2.63 goals per match,
seeds 1–40 gave round h's 3.00 (± 0.25 at 40 matches).

**Experiments** (`match_lab --experiment file.ron`, `crates/lab/src/experiment.rs`). An experiment file lists arms;
each arm is a pair of squads (builtin or a `TeamData` file) with changes (formation, replaced team instructions,
manager options `TeamOption`, player ratings by name, player instructions) and an optional `SimConfig` override. Every
arm plays the same seeds; 16 per-match metrics (points, goals, xG, shots, possession, completion, offsides, fouls,
corners) are compared by paired differences per seed with 95 % confidence intervals, each arm against the first and
any weighted contrast the file names (e.g. a 2 × 2 interaction). `--home` / `--away` also take squad files now.

The plan's example, `data/experiments/high_line_fast_strikers.ron` (500 matches per arm, 2 × 2): Northbridge with a
high block or its layer-1 block, against Harbour's strikers as they are or at speed 94.7 / quickness 89.5.

| Contrast | Points | Goals for / against | xG for / against | Shots for / against | Offsides against Harbour |
|---|---|---|---|---|---|
| High block (B − A) | +0.47 ± 0.16 | +0.88 / −0.10 | +0.84 / −0.20 | +10.1 / −3.5 | +2.1 |
| Fast strikers (C − A) | −0.16 ± 0.16 | −0.08 / +0.10 | −0.03 / +0.14 | −0.1 / +0.4 | +0.1 |
| Interaction (A − B − C + D) | +0.06 ± 0.21 | +0.05 / +0.06 | +0.06 / +0.03 ± 0.09 | +0.2 / +0.8 | 0.0 |

The high block wins the ball higher and pays nothing for the space behind it, even against fast strikers: the
interaction is nil. That is the experiment's main finding, and it matches the scorecard: counter-attacks are half as
long as real ones, and few goals come from short moves. It is left open for the report rather than calibrated here: the
cause is in how attacks into space are chosen and supported (ATT-18 runs in behind, TR-01 counters), not in the block.

**Possession value by self-play** (`--learn-value out.ron`, `crates/lab/src/value.rs`). While the ball is live the
stats note every 3 m cell (team frame, wings folded) where a team had the ball under control, once per possession;
when the possession ends, each noted cell gains a visit, and a goal if it ended in one (`Calibration::value_visits` /
`value_goals`; the possession running at full time is left out). The learned value of a cell is its share of scoring
possessions, a first-visit Monte Carlo estimate of the engine's own policy. Own-half cells see ≈ 8 goals in 300
matches, so their raw shares differed tenfold from cell to cell. Each cell therefore pools a square neighbourhood
until it holds 40 goals (radius ≤ 5 cells), and what is still thin is shrunk towards the field the matches were played
with (weight 50 possessions). The engine reads a learned field from `AiConfig::value_grid` (`ValueGrid`, bilinear
between cell centres); `None`, the default, keeps the parametric model.

Learned from 300 matches each (central lane; parametric in brackets): own third 0.004–0.005 (0.004–0.005), middle
third 0.004–0.005 at 30–60 m (0.007–0.012) and 0.012 at 60–75 m (0.026), 75–90 m 0.061 (0.067), last 15 m 0.25
(0.23). The engine's possessions gain almost nothing from crossing the middle third; real ones gain about half again
(StatsBomb possession xG to come, calibration table: 0.0105 at 25–40 m, 0.0169 at 52.5–65 m). Played with on fresh
seeds (`data/experiments/value_field.ron`, 300 matches per arm), iteration 1 adds +0.94 goals per match with fewer
shots (better chances: xG per shot 0.075 → 0.094, real ≈ 0.10) and 6.5 fewer fouls; iteration 2 (learned with
iteration 1) differs from iteration 1 by −2.4 shots and −0.11 xG against, so the iteration has not settled. On the
scorecard iteration 1 moves one target into range (shots within 10 s of a corner) and one out (direct free-kick
xG), and several further away (goals from short moves 30 → 25 %, counter length 33 → 31 m, low-block line 14.7 →
11.8 m, presses by 2–3 players 40 → 36 %, mid-block length 30.2 → 32.7 m); fouls and yellow cards drop below their
references. The parametric model therefore stays the default; the two fields are kept in `data/value` for
experiments and must be relearned after changes to the AI.

## Watch round (2026-11-01)

Matches were watched frame by frame instead of read from aggregates. `examples/film.rs` records a match as JSON lines
(a header with the squads, then one line per frame every 0.2 s with positions, velocities, targets and movement
labels of all 22 players, the ball and the phases, and one line per event); offline scripts render frames, list event
sequences and measure spells, shapes and movements against the StatsBomb events (40 matches, real counts × 1.62).

**Fixed** (each a mechanism that was wrong, not a tuning; benches in the catalog entries):

| Finding | Cause | Change |
|---|---|---|
| A 10.7 m/s roller 0.9 m from the keeper passed him untouched | a failed save left the ball's path unchanged, also through his body | `PlayerConfig::gk_body_radius` 0.3 m: a failed save that passes within it comes off him (GK-01); save % +2–4 points per band |
| 59 % of shots from beyond 22 m (real ≈ 40 %) | (a) the xG keeper race flew the ball at constant speed; (b) first-time shots were priced against the keeper where he stood at the pass; (c) 43 % of first-time shots were struck ≥ 8 m from the point they were priced at (a stale plan); (d) the egotism shot bonus was a share of the local value scale, half a long shot's xG | (a) drag-aware flight time in `analysis::keeper_open_share`; (b) `Situation::arrives_in`; (c) `att06_replan_m` 2 m: the receiver re-decides when the ball is further from the planned point (ATT-06); (d) `PersonalityConfig::egotism_shot_share` 0.5 × egotism × the shot's xG (replaces `egotism_shot_factor`). 30–40 m shots 7.2 → 3.6 per match, 22–30 m share 28 → 23 % (real 7.9) |
| 86 % of corners were ground balls to the front post | the taker's softmax priced a ground ball to a marked attacker as possession 5 m from goal | SP-02 routine drawn per kick [ACAD-C01; PL-TT24]; shot within 10 s per corner 0.29 (0.29–0.36) |
| Throw-ins were kicked along the ground | no throw | SP-06 thrown from 2.2 m at 32°, 12–19 m/s by strength [LIN-TI01] |
| Forwards held the ball ≥ 6 s 3× as often as real ones | a holder always faced the opponents' goal, so the presser goal-side of him stood in front of him and never found him closed to play; the double-up the catalog cited was not implemented | ATT-25 shielding stance (back to the nearest opponent, [USS-D]); PRS-01 double-up `DefDuty::Double` [BDFL-PETER]; forwards' spells ≥ 6 s 12.7 → 9.8 per team-match (real 3.7) |
| Box runners left their spots while the cross was in the air | the runners' passer switched to the cross's receiver inside the box, ending FT-03's "carrier wide" | `offer::Passer::cross_from` |

After these (40 matches): goals 2.38, shots 14.2 / 12.9, xG 1.22 / 1.05, scorecard 6 of 32 targets, 5 of 10
references. Two brains tests were re-measured: the wing-back test now plays four seeds (two gave 2.9–9.8 m per pair),
the coordination test compares with factor 2 (ratio 3.2 over eight seeds; history in the file).

**Open findings, grouped by cause.** Fixing findings one at a time pulled metrics against each other in earlier
rounds (the ATT-11 knots, the value curve, the receiver price), so the open findings were collected and grouped first:

- **P, both blocks too deep and too passive.** Mid-block line 31.7 m (real 37.2–37.9), length 30.3 m (22.5–29.1); low
  block line 13.9 m (17.6–18.1) and width 27.5 m (35.5–36); keeper 11.3 m from goal (11.8–14.4, he keeps the GK-03 gap
  to the line); offsides 1.5 (2.2–2.7); presses by 2–3 players 41 % (58, one team); fouls in the attacking third 8 %
  (27–33); goal kicks short 84 % (46–56, nobody presses them). `blk05_line_mid` itself sits at 32.5 m.
- **S, too few players ahead of the ball, nobody holds width.** In possession the back line is at 21 / 34 / 44 m by
  phase (19.6 / 40.3 / 53.3) and the team 41 / 46 m wide in build-up and progression (54 / 55). Wide players' targets
  are at 25–26 m from the centre line, their positions at 21–22 m: they switch movements every 0.6 s (median) and lag
  behind each new target. In the final third 1.4 team-mates are ahead of the ball and 0.22 in the box. Consequences:
  final-third support (3+ near) 6 % (54), box at a cross 0.59 (2.35), one-touch goals 12 % (63–80), goals from 1–5
  passes 29 % (48–58), shots from 22–30 m 23 % (7.9), the centre-forward tackled 18–25 times per match (dispossessed or
  failed dribbles 2.1 real), counters 32 m (63–77).
- **R, contacts too clean.** Corners 1.5, throw-ins 10, goal kicks 5.6 per team (8.1 / 26 / 12.4); clearances 4.7 (29.3).
  Corners come only from saves and blocks: no cleared or deflected cross goes behind.
- **T, tempo at the back.** 1100–1240 open-play passes per team (792); keepers and defenders hold the ball ≥ 6 s 0.8 /
  1.7 times per team-match (4.7 / 9.5). A pass every 2.3 s of possession (3.5 s real) also re-triggers every off-ball
  player's movement choice, which feeds S's churn.
- **K, keepers.** Save % at 0–6 m 58 (23); penalties 89 % scored on a small sample (78–80).
- **F, running.** Second half −12 % (−6.3 to −7.7); high-intensity running ≈ 2× real.

Interactions: P and S are coupled through the offside line (raising the in-possession lines alone gave 5.5 goals,
ATT-11); more pressure (P) means fewer easy passes (T, good) but more long balls (bad) unless S gives the pressed
defender a short forward option; better support (S) creates close chances that K's keepers save too often; R follows
partly from P. F is independent, but faster positional pacing for S would raise running that is already at the real
level (14.0 km per 90 in-play minutes against ≈ 9 km in 57) and high-intensity running that is already 2×: the width
has to come from fewer switches, not faster ones.

Experiment A1 (`blk05_line_mid` −15, the sourced 37.5 m, 40 matches): mid-block line 35.8 m, length 28.7 m, keeper
13.1 m, offsides 2.21 (all in or near range); in-possession lines +1 m; S unchanged and counters shorter (26 m: the
ball is won higher up). As expected, P alone does not fix S.

**Plan.** (A) structure: the sourced block line together with S's mechanism (movement commitment, the low block's
depth); (B) pressure: a second presser only if the more compact block does not bring it, then goal-kick pressure and
fouls high up; (C) contacts: clearance and deflection outcomes from the contact physics, not a restart bias; (D)
keepers at close range; T and F re-measured after A–C.

**Set A: block line and shape speed.** Films first (`examples/film` now takes a RON of `SimConfig` overrides and
records each player's anchor):

- The wide players' lag in possession is a transition lag, not churn. At the regain their in-possession spots are
  15–20 m ahead (winger target − position along the pitch: 20.6 m in the first 3 s, 14.5 m at 3–8 s, 8.1 m at 8–15 s,
  4.0 m after 15 s). Without movement noise, or with a switch margin of 0.2, the label switches halve and the lag
  stays; a longer check interval changes nothing (the switches come from cue changes).
- The urgency of a positional movement had its own tail (85 % only at 20 m from the point), so a winger 8 m short of
  his spot ran at 3 m/s whatever the pace time.
- Running by phase: out of possession 8.3–8.6 km against 5.7–5.8 km in possession per 90 live minutes (+46 %),
  high-intensity 2.4 against 0.7 km (+250 %); real +7 % and +30 % [JEROME23]. Every player sprinted in a counter-press
  or recovery phase: the block players' sprints were 11 % of all high-intensity running.
- The running budget: with the ball in play the rate is > 2× the rate out of play [JEROME23]; with 57.5 minutes in
  play and 10.94 km per match [ALTMANN23] that is ≈ 13–13.6 km per 90 in-play minutes. The earlier "≈ 17 km" scaled
  all running by the time in play and overstated the room.
- The low block's width and depth (finding P) are a mix effect: with the ball central 20–40 m out the back four spans
  21–26 m (6–8 m gaps [FIFA-B4] ⇒ 18–24 m), and the deepest defender given the ball's distance matches StatsBomb
  (BLK-02 bench). Not a target.
- In the final third even the targets put only 1.7–1.9 team-mates ahead of the ball and 0.3–0.5 in the box; the
  centre midfielders and full-backs are never ahead. Movements are valued as options for the current or next carrier,
  so nobody goes for the box ahead of the wide entry: a choice problem, left for the next set.

Changes: `blk05_line_mid` −20 → −15 m (BLK-05, the WC2022 mid-block line); a paced movement runs at `pace_run_speed`
while the gap is large and closes the rest in its pace time, its speed no longer limited by the urgency
(`locomotion::desired_velocity`); in counter-press and recovery only the players with a duty go unpaced, the block
gets into shape at a run (TRN-03, ATT-11: "quickly make playing area as big / small as possible" [USS-D]). Pace times
of 2 and 1.5 s were tried: +2.5–3 km of running, own-third width 43.1 → 43.4 m; kept at 4 s.

| 40 matches | before | A1 only | set A |
|---|---|---|---|
| mid-block line / length (m) | 31.7 / 30.3 | 35.8 / 28.7 | 36.0 / 28.6 (37.2–37.9 / 22.5–29.1) |
| keeper to goal line (m), offsides | 11.3, 1.54 | 13.1, 2.21 | 13.0, 2.29 (11.8–14.4, 2.2–2.7) |
| in-possession width build-up / progression (m) | 40.5 / 45.5 | 42.8 / 46.0 | 43.0 / 46.0 (54 / 55) |
| running, high-intensity (km per 90 in-play min) | 14.0, 2.88 | 13.2, 2.55 | 13.3, 2.42 (13–13.6, ≈ 1.2) |
| second half, HI last vs first 15 | −12 %, −38 % | −14 %, −36 % | −10 %, −30 % (−7 %, −20 to −45 %) |
| goals, shots per team | 2.38, 13.5 | 2.30, 12.2 | 2.33, 13.3 |
| scorecard targets, references | 6 / 32, 5 / 10 | 9 / 32, 5 / 10 | 9 / 32, 6 / 10 |

Still open from P and S: in-possession width and lines (distance at run speed after a regain; the final-third choice
problem), running out of possession +49 % over in possession in the films (real +7 %): the attack moves too little,
not only the defence too much.

**Set S2: who is ahead of the ball (2026-11-02).** StatsBomb 360 freeze frames against the films, the ball 23–32 m from
goal: real 3.6–4.0 visible team-mates ahead of the ball, 0.24–0.36 in the box, 3+ within 15 m in 32–56 % (a lower
bound, only visible players count); engine 0.94–1.06, 0.10, 1–14 %, and even the targets only ≈ 1.9 ahead. Causes:
(R1) the 4-3-3's in-possession depths made a 2-3-2-3 although its file said 2-3-5 (#8s at 0.5 / 0.64 of the length);
(R2) the ATT-11 knots sat at the centres of the thirds although the source values are phase means; (R3) movements
were valued for the current carrier only (docs/player-brain.md 8.4, level 1, was designed but not built); (R4) the
width after a regain (34 m in the first 3 s); (R5) an instruction was a fixed share of the local scale, smaller than
the spread of the movement values (`forward_runs` ±1 changed nothing at the sourced line).

Changes:
- R2: `att11_ball_x` [−32, 0, 29] m, the mean ball position of each phase (StatsBomb, 360 matches).
- R3: `offer::next_passer`: the carrier's most likely pass makes its receiver the next carrier; a movement adds what
  its offer adds to his options times the pass's probability, weighted by look-ahead, and its candidate points
  include those offered to him (ATT-26).
- R5: an instruction is a share of the movement's own worth, or of the local scale if larger.
- R1: in-possession shapes as the creation-phase shapes of UEFA-TR24 p.50 / UEFA-TR20 p.35 (five across the front):
  4-3-3 #8s 0.82 / 0.9 (upper box); 3-4-2-1 wing-backs 0.92, #10s 0.86; 3-4-3 wing-backs 0.92; 3-5-2 wing-backs 0.9
  and one #8 (0.86, 0.58). The 4-4-2 is unchanged ("width from the midfield line"); the 4-2-3-1 #10 stays at 0.72
  (0.85 puts him between the lines already, and no source number separates the two).
- Lab: the in-possession shape metrics leave out the attacking transition (PH-01's 6 s after a change of
  possession), which FIFA lists as a phase of its own; 37 % of the build-up and progression samples were in it.
- R4, unpaced expansion during the attacking transition [USS-D p.57–58], was measured and dropped: +0.7 km of
  high-intensity running, offsides 3.0, counter-attacks 36 (pace 0 reads as a counter), and the width +1 m. The width
  in the first seconds is where the players stand at the regain, not how fast they leave it.
- `expected_best` and `score_moves` reuse their option buffers (bit-identical). Unloaded a match takes ≈ 12 s with or
  without R3; the 30 s per match of a 40-match lab is 12-thread contention.

| 40 matches | set A | S2 with R4 | S2 |
|---|---|---|---|
| goals, offsides | 2.33, 2.29 | 2.88, 3.01 | 2.22, 2.45 |
| running, high-intensity (km) | 13.3, 2.42 | 14.5, 3.30 | 14.1, 2.66 |
| build-up width (m) | 43.0 | 43.6 | 47.1 outside the transition (54) |
| final-third line (m) | 45.7 | 47.0 | 47.2 (52.6–53.7) |
| 4-3-3 team-mates ahead of the ball 23–32 m out, positions / targets (films) | 1.0 / 1.9 | 1.39 / 2.11 | 1.52 / 2.78 (real ≥ 3.6–4.0) |

Two scenario tests changed: the instruction test runs at the sourced line; FT-03's
second box runner now has to take another FT-03 spot, because the far post and the penalty spot are 5.7 m apart,
inside `offer_overlap_m`, and he may value the far post for the next carrier. Still open: every role stands 4–6 m
behind its target in the final third (line 47 against a 51–52 m target); the ball reaches 23–32 m from goal with a
forward 80 % of the time, where real passes from there come from full-backs (24 %) and holding midfielders (22 %);
running out of possession is the excess (films: 8.25 km and 1.94 km high-intensity against 6.2 and 0.80 in
possession; real out = in × 1.07 and × 1.30 [JEROME23]), so in-possession running is at the real level and the lag
has to come out of churn, not speed.

**Set T: tempo on the ball (2026-11-02).** Films (seeds 11–14) and StatsBomb (40 matches): a free carrier released
at his first decision. Reception → pass median 0.48 s (real 1.22), 41 % of releases 0.3–0.7 s after the reception
(real 8 %), spells over 2 s 13 % (28 %), 1095–1279 passes per team (792). Causes: (T1) the continuation of keeping the
ball was the best pass now with a 1 s half-life, always below playing it now; (T2) the softmax gave every team-mate
his own weight, so "pass to someone" outweighed keeping the ball however much keeping it was worth; (T3) recovery runs
read an opponent's pass in flight as an attack at the ball's speed (a 15 m/s pass put the recovery point 33 m away
and made the run a sprint); (T4) the settle law's scan factor also shortened getting to the ball.

Changes:
- T1, look-ahead level 1 for the carrier (docs/player-brain.md 8.4, `on_ball::ahead`): holding and carrying are
  worth the best pass he expects at his next decision (`onball_interval` when holding, the carry time when carrying),
  priced on a copy of the state where team-mates go towards the offers he reads (`reading::seen`) and everybody else
  carries on with his velocity; scaled by (1 − pressed) as before. `continuation_halflife_s` is gone.
- T2, ATT-01's frame [USS-D: shot, pass or dribble]: the passes together get the weight of the best pass, shared
  among the team-mates in proportion to their own (as the shot variants, the ways to one team-mate and the dribble
  directions already were).
- T3, IND-06: against an opponent's pass in flight the recovery trigger, point and pace are judged from its
  reception point and arrival time (as PRS-02 judges the first defender's), at `pace_ball_speed` from there.
- T4: settling = `settle_decision_s` + scan factor × `settle_composure_s` × (1 − composure).
- Exposed by the longer spells and added to the set: IND-03's "about to get past him" uses the carrier's speed
  relative to the defender's (one giving ground as fast still contains); DEF-03's hysteresis now holds for the second
  defender as the catalog says (it was re-picked every tick).

| 40 matches; films 11–14 | S2 | T1 | T1 + T2 | set T |
|---|---|---|---|---|
| reception → pass median / mean (real 1.22 / 1.59 s) | 0.48 / 0.88 | 0.56 / 1.00 | 0.72 / 1.47 | 0.72 / 1.47 |
| > 1 s / > 2 s / > 3 s (59 / 28 / 15 %) | 24 / 13 / – | 29 / 15 / 7 | 40 / 24 / 14 | 40 / 24 / 14 |
| first time (18 %) | 25 | 27 | 23 | 21 |
| passes per team (792) | 1095 / 1279 | 1020 / 1211 | 885 / 999 | 872 / 1000 |
| keepers' free spells < 0.3 s (12 %) | 51 | 30 | 26 | 27 |
| goals; shots per team | 2.22; 15.3 / 11.6 | 1.80; 16.4 / 12.1 | 3.00; 23.0 / 15.6 | 2.65; 20.2 / 15.6 |
| tackles per team; fouls; yellow cards (≈ 25; 19.9; 3.5) | 68; 18.7; 3.6 | 74; 19.2; 3.6 | 99; 24.7; – | 96; 23.1; 4.6 |
| out of possession km / HI per player-90 (≈ 6.7 / 0.6) | 8.26 / 1.97 | 8.01 / 1.78 | 7.95 / 1.69 | 8.10 / 1.72 |
| recovery run: target distance, speed | 32.9 m, 4.72 m/s | 24.5, 4.40 | 24.2, 4.39 | 24.0, 4.39 |
| out-of-possession label switches per player-minute | 32.9 | 31.9 | 31.1 | 29.9 |

Carries (path median, duration; StatsBomb carries within spells): pressed midfielders 4.1 m / 1.4 s (real 4.2 / 1.5),
pressed forwards 6.2 / 2.2 (4.8 / 1.7), but free defenders 1.3 m / 0.6 s (3.9 / 1.7) and free midfielders 1.4 / 0.6
(4.1 / 1.6): a free carrier still passes instead of carrying first. The longer spells cost duels: carries are tackled
far more often than real (IND-03 last ditch 68 and loose touches 60 per match, touches 1.6 m out with the defender in
front in half of them; the old "carry in space loses the ball 2–10× as often per second" finding), so tackles, fouls
and cards rose. Next set: duels on the carry. Single-threaded seconds per match in the 40-match lab 28 → 41 (more
decisions per spell, each with two look-ahead pass evaluations).

**Set C: what a pass and a loss are worth (2026-11-02).** The set started as "duels on the carry" and turned out
to be about the AI's prices.
- The AI raced a pass against every opponent near its path as independent chasers, each starting from a standstill
  when the passer decided; in the sim one opponent goes, and he reacts at the release.
- The value curve was steep in the own half, and a loss was priced at the opponents' mirrored receipt value, its
  shot term included, then weighted 1.4 / 1.0 / 0.6 by third. A loss 10–20 m from the own goal cost 0.08–0.17 xG
  against the real 0.02–0.03, and one in the final third 0.6 of the real cost.
- Decision noise was a quarter of the value scale; three quarters of the risky middle-third passes were not the
  best pass, while a safe one was on offer.

Changes, applied as one set (inventory and reasoning in the watch findings, C1–C26):
- G0, IND-08 / PRS-02 on the passer's side (`analysis::race`, `evaluate_pass`):
  - Opponents start at the release, carrying on with their movement through the set-up (`opponent_start`,
    `locomotion::carry_on`).
  - A low ball that reaches an opponent within his reaction time hits him (`block`).
  - The best-placed opponent is the one chaser.
  - The receiver's pressure clocks start at the release too.
- G1: the fitted value curve becomes the default (above: base 0.0083, progress 0.028, power 1.84, width 39 m, shot
  share 0.31).
- G1b, ATT-02: a loss costs the xG of the possession the opponents' regain starts there (`analysis::turnover_cost`;
  `regain_base` 0.004, `regain_progress` 0.030, `regain_power` 0.8, `regain_width` 35 m, fitted to 11,022 StatsBomb
  regains). The per-third weights are 1 / 1 / 1; the source's "more risk further up" is in the gradient.
- G1c, ATT-17: a ball in behind the last line is worth `value_behind` 0.040 × exp(−(y / 10)²) more than the place
  (`analysis::value_at`, `last_defender`, StatsBomb 360: central receptions with no outfield opponent goal-side).
  - It is used wherever possession at a point is valued: passes, offers, carries, holding, the carrier's look-ahead,
    egotism's "better placed" team-mate and the markers' threat.
  - The last line's factor in the line-break bonus drops from 2 to 1.
- G1d, ATT-24 in the AI: a pass is worth what the ball is worth where the receiver gets it, and breaks the lines in
  front of that point (`PassEval::recv_point`):
  - free, where `Simulation::reception` has him take it (the reachable point nearest the target);
  - pressed, where he meets it first;
  - in between, blended by his pressure.
  Until then a through ball he met short of the line was priced as the ball in behind.
- G3: `temperature_worst` 0.23 → 0.115.
  - Arms 0.23 / 0.115 / 0.06 gave ground 5–40 m completion 89 / 92 / 92 %.
  - Completion with the passer's nearest opponent 1.5–3 m away was 89.2 / 93.3 / 92.8 % (real 92.3).
  - Below 0.115 nothing moved.
- `positioning::choose_move`: movements whose points are within `SAME_PLACE_M` (1 m) share one noise draw, so a
  place does not win more often for having two names (hold and pin the line).

| 40 matches; benches 4 | set T | G0–G3 + G1c | set C (+ G1d) | real |
|---|---|---|---|---|
| goals; shots per team | 2.65; 20.2 / 15.6 | 2.80; 20.6 / 16.0 | 2.88; 19.5 / 13.2 | sanity ≥ 2.47 |
| ground passes 5–40 m completed | 87 % | 89 % | 93 % | 95.8 % |
| by the passer's third: own / middle / final | – | 96.3 / 89.9 / 67.3 % | 98.5 / 92.8 / 79.3 % | 97.4 / 97.0 / 91.2 % |
| passer's nearest opponent 1.5–3 m / 8 m + | 87.3 / 97.5 % | 90.6 / 96.3 % | 93.6 / 98.4 % | 92.3 / 99.1 % |
| interceptions; tackles per team | 120; 96 | 89; 79 | 63; 74 | –; ≈ 25 |
| middle-third passes chosen at p < 0.8 per match | – (C5: 253) | 192 | 93 | – |
| through balls per team-match (completed) | – | 50.5 (48 %) | 8.4 (63 %) | 2.0 (35 %), definition differs |
| receptions beyond the last line per team-match | – | 1.9 | 3.2 | 7.6 |
| pressed (< 1.5 m) holder: loss hazard; time on the ball | 0.13 /s; 2.60 s | 0.19 /s; 1.96 s | 0.26 /s; 1.55 s | 0.28 /s; 0.82 s |
| take-on: AI keep vs outcome | 0.74 vs 0.92 | 0.85 vs 0.91 | – | – |
| fouls; yellow cards | 23.1; 4.55 | 20.4; 3.88 | 18.4; 3.58 | 20.0; 3.49 |
| goals from 1–5 passes | 35 % | 39 % | 25 % | ≈ 53 % |
| scorecard (situation-conditional) | 8 / 32 | 7 / 32 | 8 / 32 | – |

Still open:
- The final third holds 4.8 % of the passes (real 22 %), completed 79 % (91 %); the team's line there is at 47 m
  (52.6–53.7).
- Counters cover 25 m in 8.6 s (70 m in 9–11 s). Quick goals fell when the through-ball artefact went, because the
  runs they came from are still late:
  - strikers start 110 runs in behind per match (real ≈ 26.5);
  - they are on the line in 11–17 % of the samples while their targets are on it in 48–57 % (positional lag);
  - at three in four passes 10–60 m from goal no team-mate is on the last line (real one in three).
- One test is deferred to the next set (`att26_when_one_striker_comes_short_the_other_holds_the_depth`): with the
  flatter curve the second striker's run in behind gains about what it costs in effort, so he comes short half the
  time. The effort and noise of the movement layer are still scaled for the old curve.

Next set (S): in-possession positions and runs. That covers the positional lag, run churn, final-third occupation,
the movement layer's scale and the counters. Duels on the carry come after it (tackles ≈ 3× real).

## Set S: in-possession positions and runs (2026-11-02)

Diagnosis (films of 4 seeds, ablations one mechanism at a time; S/watch/findings.md, "Set S inventory"):
- Pacing never arrived: speed = gap / `pace_position_s`, re-evaluated every tick, decays exponentially, so a 2.4 m gap
  closed at 0.6 m/s. A front player stood 8.4 m short of the last line while his point was 4.8 m short; strikers
  were on the line in 10–15 % of the samples, their points in 34–60 %.
- The movement layer's misjudgement was drawn afresh at every trigger (new carrier, pass struck, carrier turning
  open or closed): 27 re-choices per player-minute in possession, and the noise decided as much as the values did.
  Without it the label changed 15 instead of 25 times a minute and pinning the line won five times less often.
- Most of the lag is the transition: a striker is 13 m behind his point in the first 3 s after a regain, 9 m at
  3–8 s and 5 m after 15 s (a winger 19 / 14 / 9 m), as the positional moves jog up at the pace law.
- Neither noise nor pacing changes the settled gap much (strikers ≈ 6 m, wingers ≈ 11 m short); a winger's point is
  itself 5 m short of the line by design.

Changes:
- `pace_min_speed` 1.4 m/s: a paced movement walks at least until it brakes at its point (`locomotion`, and the
  carrier's prediction of a team-mate's offer in `on_ball::ahead`).
- `movement_noise_period_s` 6 s: in possession the misjudgement drifts (successive draws blended with a per-player
  phase, unit variance) instead of being redrawn at every trigger (`positioning::choose_move`).
- TR-01 "intense forward runs": on the counter a player going to a point ahead of the ball and of himself runs at
  `tr01_run_urgency` (1.0) × (0.8 + 0.2 × work rate), unpaced, until `tr01_line_margin` (5 m) short of the last line.
  Without the margin offsides rose from 3.1 to 3.7 per team.
- Tried and dropped: feeding the point's velocity forward (× look-ahead): lag 3.5 m against 3.6.
- Lab: the counter-attack rows compared the TR-04 phase (every counter) with UEFA's single counter goals (70 m in
  9–11 s). They are replaced by a like-for-like row from StatsBomb (609 matches): of the open-play shots of
  possessions begun by a regain, the share within 12 s by the regain's third (`Calibration::regain_shots`).
- The ATT-26 two-striker test's baseline is fixed: the first striker now pins the line for the "alone" case (he had
  checked by himself while the scene settled). It still fails for a reason of its own: his check, 12 m across, is a
  separate option for the carrier, so the second striker's check adds 0.0019 either way. Holding the depth for a
  team-mate who comes short has no value beyond the offers' overlap (open, finding S12).

| 40 matches; films 4 seeds | set C | set S | real |
|---|---|---|---|
| goals; shots per team | 2.88; 19.5 / 13.2 | 3.03; 19.1 / 18.5 | sanity ≥ 2.47 |
| strikers on the last line, position (their point) | 15 % (60 %) | 21 % (60 %) | – |
| in-possession re-choices per player-minute | 26.8 | 20.1 | – |
| runs in behind started per striker-match | 110 | 94 | ≈ 26.5 |
| regain → shot within 12 s: own / middle / final third | 1 / 23 / 88 % (dump) | 4 / 32 / 94 % | 13 / 37 / 71 % |
| shots after own-third regains per match (× 1/1.62) | 3.1 | 4.3 (sA5 dump) | 5.2 |
| attacks covering ≥ 50 m to a shot per match (× 1/1.62) | 3.6 | 5.6 (sA5 dump) | 5.9 |
| goals from 1–5 passes; one-touch goals | 25 %; 13 % | 35 %; 24 % | ≈ 53 %; 63–80 % |
| offsides per team | 2.80 | 2.88 | 2.21–2.70 |
| running per player-90 in play: km in / out; HI in / out | 5.78 / 7.90; 0.68 / 1.59 | 6.34 / 8.40; 0.82 / 1.69 | 13–13.6 km in all |
| scorecard (situation-conditional) | 8 / 32 | 9 / 33 | – |

Still open:
- Running: 14.4 km per player-90 in play (13–13.6). The excess is out of possession, where duties churn
  (block ↔ mark ↔ cover, 28 switches per player-minute; 43 % of the high-intensity bursts start within 0.6 s of
  one). The pace floor adds to it, since the defenders' moves now arrive too.
- Turnovers: about 230–245 changes of control per match in the films against 111 real (68.8 × 1.62), most of the
  excess in midfield (30–75 m from the goal attacked: 151 against 55). That is why 94 % of the shots after
  final-third regains come at once, and why there are 7.2 of them per match against 1.3. It is also why own-third
  regains rarely become fast attacks. Interceptions (≈ 69 per team) and tackles (≈ 71) are 3–4× real.
- Wingers stay 11 m short of the line and the widths stay at 48 / 49 m (54–55).
- The final third holds 4.8 % of the passes (22 %).

Next set (D): out-of-possession duties and duels. That covers the duty churn and its running, the engagements and
tackles on the carry, and interceptions as far as positions cause them (lanes). Turnovers are measured by zone
against StatsBomb's regains.

## Set D: the pressed carrier (2026-11-02)

Inventory first (S/watch/findings.md, "Set D inventory"), grouped by cause before any change: (P) a pressed carrier
is too safe, (W) play is too central and rarely reaches the final third, (A) contests are clean (an opponent's first
touch is always a controlled interception, no first-contact clearances), (R) duty churn and running out of
possession. The groups do not pull against each other; (P) came first because it also shortens the contact that
the tackles, fouls and take-ons come from.

Diagnosis of (P): a carrier lost the ball only to a tackle. With an opponent within 1.5 m at the reception he lost it
at 0.146 per second on the ball (StatsBomb 360: 0.28), 92 % of the losses tackles (53 % tackles, 42 % miscontrols),
and he kept it 1.90 s (median; real 0.82 s). The AI knew it: carrying was worth as much as the best pass under
pressure, so the forced commits (IND-03 last ditch, the loose-touch commit on a knock-on that ran to the defender)
piled up over the contact time; take-ons ran at 44 per team (real 21.3).

Changes:
- `duel::pressed_loss_hazard`: a carrier under pressure (`pressure_at` at the ball) loses the ball by himself at
  `pressed_loss_rate` (0.3 per second) × pressure² × (1.3 − 0.6 × skill), skill 0.6 dribbling + 0.4 composure. The sim
  (`pressed_fumble`) applies it while he settles the ball, shields, dribbles or sets up a kick (not in a keeper's
  hands): the ball squirts 1.5–5 m off his foot (a dribbler's along his line ± 70°, a shielding player's any way), a
  `Miscontrol` event. The on-ball AI prices the same hazard: holding keeps exp(−hazard × `onball_interval`), a dribble
  exp(−mean hazard at its start and end × its duration).
- `dribble_touch`: a knock-on runs no further than `touch_gap_share` (0.5) of the gap to the tackle reach of an
  opponent near its path ahead, so the carrier gets to it first (finding C2: the knock-on ignored the defender).
- Swept 0.15 / 0.3 / 0.5 on the `receptions` bench: the miscontrol share of the losses within 1.5 m went 20 / 30 /
  31 %, while losses in the 5–8 m band (already above real) rose to 48 % miscontrols at 0.5. 0.3 is kept.

| 40 matches; benches 4–8 matches | set S | set D | real (× 1.62) |
|---|---|---|---|
| goals; shots per team | 3.03; 19.1 / 18.5 | 2.83; 17.9 / 14.8 | – |
| tackles per team | 72 / 70 | 64 / 67 | 25.3 |
| take-ons per team: successful + lost (success) | 26.0 + 17.9 (59 %) | 19.1 + 15.8 (55 %) | 21.3 (51 %) |
| losses to a heavy touch or fumble per team | 3.2 | 17.6 | miscontrols 17.3 |
| receptions with an opponent within 1.5 m: losses per second; miscontrol share; median time kept | 0.146; 8 %; 1.90 s | 0.217; 30 %; 1.60 s | 0.28; 42 %; 0.82 s |
| same, 1.5–3 / 3–5 / 5–8 / 8+ m: losses per second | 0.100 / 0.065 / 0.033 / 0.006 | 0.110 / 0.076 / 0.034 / 0.010 | 0.14 / 0.056 / 0.016 / 0.003 |
| commits per match: last ditch; loose touch | 56.8; 57.5 | 62.2; 37.5 | – |
| open-play regains per match: tackle / interception / loose touch | 73 / 82 / 6 (197 in all) | 60 / 82 / 35 (215) | 105 in all |
| throw-ins per match (outs bench) | 19.8 | 23.5 | 53.5 |

Still open:
- Regains rose, since the fumbles add to the tackles more than they replace them. The per-situation hazard is near
  real within 3 m; the excess is in the 3–8 m bands (a receiver who had space loses it 1.4–3× as often per second as a
  real one) and in how often a carrier is pressed at all: defenders close free receivers fast (high-intensity running
  out of possession 3× real, D2) and commit (last ditch 62 per match).
- Under pressure the choice stays spread: in later pressed decisions the best pass is the best option on the median
  (+0.0110 against +0.0095 for carrying or holding), but the softmax gives passing 29 % of the mass and keeping the
  ball (dribble or hold) the rest, as the temperature rises with pressure and the utilities differ by less than it.
- Interceptions: 82 regains per match, but by every opponent first touch on a pass, which real data split into
  interceptions (11 %), blocks (18 %), recoveries (17 %) and clearances (15 %), with the ball often going out. The
  engine's ground passes 5–40 m reach an opponent first 86 times per match against 64 real (× 1.48), 1.3×; the larger
  difference is that every such touch is a clean control (group A).

Next set: width and the final third (W) with first contacts (A).

## Open after the plan

All milestones of the plan (M0–M8) are done. Statuses per principle are in the catalog; the open items are listed in
the sections above (M4 crosses, M6 benches, M7 restarts, M8 scorecard and experiments).

# Attribute palette

Status: **agreed** (2026-10-30, all questions in section 7 decided), **implemented** as player-brain phase 0 (section 8). Requested in the review of [player-brain.md](player-brain.md):
the palette should be a minimal set. For example, *intelligence* stands for tactical understanding and for how
quickly a player gets to know new team-mates and formations, and *instinct* stands for anticipation and choosing the
right decision.

## 1. Why

`player/attributes.rs` has 40 ratings plus height, weight and preferred foot. Three problems:
- **Unused ratings.** Six have no effect: stamina, marking, positioning, aerial reach, command of area, and weight.
- **Ratings that only work together.** Many always act in the same mechanism with fixed weights. Examples:
  - first touch, technique and composure in `control_skill`;
  - acceleration and agility in the first steps;
  - technique, which joins almost every kick and touch with a fixed share, so the engine can't tell it apart from
    the skill it joins.

  Separate ratings there add data, not behaviour.
- **Position know-how stored as talent.** "Positioning", "off the ball" and "marking" are knowledge of a job: where
  a defender stands, when a forward runs. The player-brain design gives that knowledge a better home: the concept
  (layer 1), the role, and the player's **experience** in that role. A striker playing centre-back reads the game
  as well as ever, but he doesn't know the defender's job.

## 2. Principles

1. **One rating, one distinct capacity.** Each rating acts through its own mechanism. Two skills get separate
   ratings when real players are commonly good at one and poor at the other. The review's example: many players
   cross badly but pass short excellently. Skills that rise and fall together, or that the engine cannot tell apart,
   are merged.
2. **Player types must survive.** Runner, target man, link player, poacher, stopper, ball-playing centre-back,
   organiser, destroyer, playmaker, tidy passer, crosser, long-range shooter, box-to-box, dribbler, egotist,
   shot-stopper and sweeper-keeper must each still be expressible (section 5).
3. **Know-how is experience, not a rating.** Familiarity with a role, with team-mates and with the system is a state
   that grows with minutes and practice. Intelligence sets how fast it grows.
4. **Facts, ratings, states** are kept apart:
   - facts: height, weight, foot;
   - ratings: 0–100, changing slowly over a career;
   - states: confidence, energy, familiarity, changing within a match or a season.
5. **Character is not ability.** Personality ratings bias choices and willingness; they don't make actions succeed.

## 3. The palette: 28 ratings

### Body (5)

| rating | replaces | influences |
|---|---|---|
| **speed** | pace | top speed: races over distance (runs in behind, recovery runs, chasing a ball), carrying speed |
| **quickness** | acceleration, agility, the lateral part of balance | acceleration, deceleration and lateral acceleration: first steps in short races, separation in take-ons and checks, turning with the ball (with dribbling), the keeper's dive (with reflexes) |
| **strength** | strength, balance | contact: shielding and holding up, the contact part of tackles and aerial duels, staying on the feet in take-ons, pushing in crowds; kick power (with the kick's own skill) |
| **jumping** | jumping reach | vertical reach with height: aerial duels, the keeper's reach for high balls |
| **stamina** | stamina (unused today) | energy drain and recovery over the match, and so how fast body, technique and mind fade (M6) |

### Technique (10)

| rating | replaces | influences |
|---|---|---|
| **first touch** | first touch, the control part of technique | receiving: the control probability of every reception, above all pressed and high balls (ATT-25 losses); settling the ball; part of shielding |
| **dribbling** | dribbling, the close-control part of technique | carrying: touch error and carrying speed, take-ons (with quickness and strength), turning with the ball (with quickness), shielding (with strength) |
| **short passing** | passing, the kick part of technique | ground passes up to 20 m at full weight, blended into long passing between 20 and 35 m (ATT-07 puts the long category at 30 m; the blend avoids a cliff). Covers ground through balls, one-twos, cut-backs along the ground and the keeper's short distribution |
| **long passing** | passing, the kick part of technique, the keeper's kicking | ground passes beyond 35 m and every lofted pass that is not a cross: switches, diagonals, balls over the top, clearances, goal kicks and punts |
| **crossing** | crossing | deliveries into the box from the wide channels of the final third, in the air or driven, low crosses included. A cut-back along the ground counts as short passing |
| **finishing** | finishing, the striking part of technique | shots up to 12 m at full weight, blended into long shots between 12 and 22 m (replaces today's hard switch at 18 m): placement, first-time finishes, volleys |
| **long shots** | long shots, the striking part of technique | shots beyond 22 m: power (with strength), dip and curl |
| **heading** | heading | accuracy and power of headed passes, shots and clearances; the skill part of aerial duels |
| **tackling** | tackling, marking | tackle reach, timing and success; poking the ball away in a duel. Where to stand is know-how (concept + role experience), not tackling |
| **weak foot** | weak foot | execution with the weaker foot, which sides and options are open to him |

### Mind (4)

| rating | replaces | influences |
|---|---|---|
| **intelligence** | tactical awareness, vision, positioning, off the ball | **understanding**: the reading term of `u_ij` (player-brain section 7), positional reliability (drift), how fully he can apply instructions (with discipline). **Match reading**: look-ahead depth, i.e. the weight of level-1 and level-2 values (third-man runs, decoys, the space a pin opens). **Vision**: which options he sees, the vision cone, scanning (M6). **Learning**: how fast role, partner and system familiarity grow and how fast new instructions become routine |
| **instinct** | anticipation, decisions | **anticipation**: reaction time, interceptions and touches on passing balls, blocks, how early he projects the ball, the carrier and opponents (look-ahead in seconds), the timing of runs and tackles, a poacher's sense for where the ball will drop. **Decisions**: the choice temperature on and off the ball. Keeper: come out or stay |
| **composure** | composure, concentration | **pressure**: execution, control and decision quality when pressed (as now), settling time after control. **Focus**: lapses over the match and late in games (M6). **Stability**: how far confidence swings after events (M6) |
| **leadership** | — (new) | **calls**: how often and how well he makes them (player-brain section 7, *Calls*). **Organising**: the claim on the line-leading duty. **Lift**: how his confidence and calls steady team-mates (M6) |

### Character (5)

| rating | replaces | influences |
|---|---|---|
| **work rate** | work rate | willingness to run: the effort cost in the movement value (player-brain section 8.5), pressing and tracking back, how readily he takes a running duty |
| **discipline** | discipline | adherence to instructions and role duties (with intelligence and practice): holding the rest-defence duty, staying in shape; fouls and cards (M7) |
| **egotism** | egotism, teamwork (as its opposite) | one axis, from team player (0) to egotist (100). On the ball: bias to shoot, dribble and hold, against passes to better-placed team-mates. Off the ball: discounts space value for team-mates and decoy runs, and favours offers for himself. Resists "pass more" instructions |
| **flair** | flair | bias to take-ons, curled shots, chips and through balls, and to movements off the concept's script. That makes him harder to read for opponents and for team-mates who don't know him yet |
| **aggression** | aggression, bravery | commitment: tackle intent, pressing intensity, going into aerial duels and blocks, risk appetite under pressure; fouls (M7) |

### Goalkeeping (4)

| rating | replaces | influences |
|---|---|---|
| **reflexes** | reflexes | shot stopping: reaction to the shot and dive reach (with quickness and height) |
| **handling** | handling | catch or parry, where rebounds go |
| **one-on-ones** | one-on-ones | the direct duel when an attacker is through on goal or carries the ball into the box: when and how far to come out (GK-02, with instinct), staying big or going to ground, smothering at his feet, blocking close-range shots, not being rounded |
| **aerial command** | aerial reach, command of area (both unused today) | claiming crosses and high balls, coming off the line for them (GK-04, M4) |

The keeper's positioning (GK-01, GK-03) comes from instinct, intelligence and role experience. Distribution uses
short and long passing.

### Facts

- **Height:** reach in the air, the keeper's dive.
- **Weight:** challenges, aerial and on the ground, with strength: aerial duels, tackles, shoulder-to-shoulder
  challenges, holding off a challenger. It plays no part in locomotion or in the soft separation between players
  (unused today).
- **Preferred foot.**

## 4. States and experience (not ratings)

| state | range | changes with | used by |
|---|---|---|---|
| confidence | 0…1 | events in the match (M6), damped by composure | effective values, reliability, calls |
| energy | 0…1 | running, with stamina | effective values |
| role familiarity | 0…1 per role | minutes in that role; the higher his intelligence, the faster it grows | reading, adherence, role know-how. Replaces `attunement` |
| partner familiarity | 0…1 per pair | minutes played together; the rate comes from both players' intelligence | reading `u_ij`, knowing a team-mate's abilities, receiving calls |
| system practice | 0…1 per team, for formation and concept | training (manager game); the rate comes from the squad's intelligence | the expectation in reading, adherence |
| instruction load | 0…1 per player | rises when instructions or the role change, falls as they become routine (the rate comes from intelligence) | lowers reading and call capacity (player-brain section 7, *Calls*) |

## 5. Player types still expressible

| type | high | low |
|---|---|---|
| runner 9 | speed, quickness | strength |
| target man | strength, jumping, heading, first touch | speed, quickness |
| link 9 / false 9 | first touch, short passing, intelligence | — |
| poacher | instinct, finishing | work rate, long shots |
| long-range shooter | long shots, strength | — |
| stopper | strength, jumping, heading, tackling, aggression | short passing |
| ball-playing centre-back | long passing, short passing, first touch, composure, intelligence | — |
| organiser | leadership, intelligence | — |
| destroyer | tackling, instinct, work rate, aggression | long passing |
| tidy passer | short passing, first touch, composure | crossing, long shots |
| deep playmaker | short passing, long passing, intelligence, composure | speed, tackling |
| crossing full-back / wing-back | crossing, stamina, speed | — |
| box-to-box | stamina, work rate, speed | — |
| dribbling winger | quickness, dribbling, flair | strength |
| egotist | egotism (often flair, finishing) | — |
| shot-stopper | reflexes, handling | one-on-ones, aerial command |
| sweeper-keeper | one-on-ones, aerial command, quickness, short passing | — |

## 6. Conversion of existing data

New ratings from the old ones, weighted by their current use, so that the benches move as little as possible:

| new | formula |
|---|---|
| speed | pace |
| quickness | 0.5 acceleration + 0.5 agility |
| strength | 0.7 strength + 0.3 balance |
| jumping | jumping reach |
| stamina | stamina |
| first touch | 0.8 first touch + 0.2 technique |
| dribbling | 0.8 dribbling + 0.2 technique |
| short passing | 0.7 passing + 0.3 technique (today's ground-pass skill) |
| long passing | 0.6 passing + 0.4 technique (keepers: kicking) |
| crossing | 0.7 crossing + 0.3 technique |
| finishing | 0.75 finishing + 0.25 technique (today's shot skill) |
| long shots | 0.75 long shots + 0.25 technique |
| tackling | 0.7 tackling + 0.3 marking |
| intelligence | 0.5 tactical awareness + 0.2 vision + 0.15 positioning + 0.15 off the ball |
| instinct | 0.25 anticipation + 0.75 decisions (the choice temperature is its sensitive use; section 8) |
| composure | 0.7 composure + 0.3 concentration |
| leadership | 58 (the scale mean) until the squads carry it |
| egotism | 100 · today's effective egotism (egotism × (1 − damping × teamwork)) |
| aggression | 0.6 aggression + 0.4 bravery |
| aerial command | 0.5 aerial reach + 0.5 command of area |
| heading, weak foot, work rate, discipline, flair, reflexes, handling, one-on-ones | unchanged |

Code that changes: the effective values (`player/effective.rs`), the duel skills (`duel.rs`), execution skill
(`ai/execution.rs`), personality (`ai/personality.rs`), reliability (`ai/deviation.rs`), adherence
(`team/tactics.rs`), the ATT-26 profiles (removed by the player-brain design anyway), the team files and the viewer
panel. This is its own step before player-brain phase A (player-brain section 16), checked on the benches.

## 7. Open questions

1. **First touch vs dribbling, finishing vs long shots: split (decided).** By the criterion in principle 1 they
   get separate ratings. Many players receive superbly but beat nobody, and many finish well in the box but cannot
   strike from distance. Merging both pairs would give 26 ratings. The user kept the split: 28 ratings.
2. **Know-how as experience (decided).** Defensive positioning and attacking movement come from intelligence × role
   experience, not from their own ratings. The higher his intelligence, the faster experience in a role grows.
3. **Weight (decided):** in challenges only, aerial and on the ground (section 3, *Facts*).

## 8. Implementation (phase 0, 2026-10-30)

`player/attributes.rs` holds the 28 ratings and the three facts; `GROUPS` lists them by group for the viewer and the
benches, and `rating` / `rating_mut` look one up by name. The squads were converted with the section 6 formulas.
`PersonalityConfig::teamwork_damping` is gone (egotism is one axis), and `bravery_risk_reduction` is now
`aggression_risk_reduction`.

Where each rating acts now (eff = the effective value with confidence and energy, `phys` the physical curve):

| mechanism | formula | file |
|---|---|---|
| top speed | speed | `player/effective.rs` |
| acceleration, braking, lateral acceleration | quickness | `player/effective.rs` |
| reaction time | instinct | `player/effective.rs` |
| kick power | 0.5 strength + 0.5 max(long passing, long shots) | `player/effective.rs` |
| tackle reach | tackling (keepers too) | `player/effective.rs` |
| jump reach | height + jumping | `player/effective.rs` |
| keeper dive reach | 0.5 quickness + 0.5 reflexes | `player/effective.rs` |
| turning with the ball | 0.5 quickness + 0.5 dribbling | `player/effective.rs` |
| challenge strength | strength + `weight_strength_per_kg` (1.0) × (weight − `weight_ref_kg` (75 kg)) | `duel::challenge_strength` |
| tackle skill | 0.5 tackling (keeper: one-on-ones, the smother) + 0.2 challenge strength + 0.15 instinct + 0.15 aggression | `duel::tackle_skill` |
| keeping the ball against a tackle | 0.55 dribbling + 0.2 quickness + 0.25 challenge strength | `duel::take_on_skill` |
| shielding (on-ball pricing) | 0.7 challenge strength + 0.3 dribbling | `ai/on_ball.rs` |
| aerial duel | 0.4 heading + 0.25 jumping + 0.2 challenge strength + 0.15 aggression | `duel::aerial_skill` |
| control | 0.85 first touch + 0.15 composure | `duel::control_skill` |
| touch on a passing ball, shot leg-block | instinct | `duel::pass_touch_prob`, `sim.rs` |
| save skill | (0.45 − 0.3 c) reflexes + 0.25 handling + (0.15 + 0.3 c) one-on-ones + 0.15 quickness; c runs from 0 at `save_one_on_one_far` (16.5 m) to 1 at `save_one_on_one_near` (8 m) from goal | `duel::save_skill` |
| ground pass | short passing up to `short_pass_full_m` (20 m), blended into long passing by `long_pass_full_m` (35 m) | `ai/execution.rs` |
| lofted pass, clearance (keepers too) | long passing | `ai/execution.rs` |
| cross (ground or lofted, `pitch::is_cross`) | crossing | `ai/execution.rs` |
| shot | finishing up to `finishing_full_m` (12 m), blended into long shots by `long_shots_full_m` (22 m) | `ai/execution.rs` |
| shank chance | `shank_probability` × (1 − the kick's skill) | `ai/execution.rs` |
| curled-shot option | 0.5 flair + 0.5 long shots | `ai/on_ball.rs` |
| choice temperature | instinct | `ai/on_ball.rs` |
| vision cone | intelligence | `ai/analysis.rs` |
| tactical reliability (drift, noise, lag) | intelligence | `ai/deviation.rs` |
| ATT-26 offer weights | run (speed + quickness + intelligence) / 3, hold (2 strength + first touch) / 3, link (2 short passing + intelligence + first touch) / 4 | `ai/positioning.rs` |
| egotism bias | egotism | `ai/personality.rs` |
| risk appetite under pressure | aggression | `ai/personality.rs` |
| adherence | discipline, egotism against "pass more" | `team/tactics.rs` |

`pitch::is_cross`: struck in the attacking third from outside the penalty-area lines (the wide channels of ATT-12)
into the box; a ground ball played back from there is a cut-back and uses short passing.

**Benches** (2026-10-31, before → after; `match_lab` 40 matches, the gap tables 8 matches each, `styles` 6 per
archetype):
- With uniform ratings (the `layers` gap tables 68 v 47, 58 v 47 and 68 v 68) drift, the layer-1 best-choice rate,
  completion and the AI's p are unchanged. The formulas carry over without a shift.
- The squads were written with anticipation well above decisions (≈ 65 vs 54–56). With instinct = 0.5 / 0.5 the
  choice temperature fell and shots rose by 12 % (21.2 / 19.5 → 23.6 / 22.1, goals 2.65 → 2.90). Ablation: instinct =
  old decisions gave 21.4 / 20.3, instinct = old anticipation 25.5 / 22.3, and intelligence = old tactical awareness
  changed nothing. The temperature is the sensitive use; the anticipation side (touches on passing balls, reaction)
  hardly moves shots. Hence the 0.25 / 0.75 conversion.
- Final: shots 22.4 / 20.1, goals 2.75, completion 81 % / 82 %, tackles 91 / 99, interceptions 179 / 172 (−3 to −4 %),
  mid-block length 29.4 m (29.9), pressers, counter-press, counter-attacks and keeper depth unchanged. `styles`
  moves within its noise.

Wired since: stamina (energy drain, M6 `player::fatigue`), leadership (how many calls a player makes and the
confidence lift he gives team-mates near him, M6 `reading::call_rate`), aerial command (how late the keeper still comes
for a cross, GK-04). Not wired yet: instinct and one-on-ones in the keeper's decision to come out (GK-02 still uses its
distance rule; one-on-ones decides the duel once he is there). The sample squads carry leadership 58 for everyone, so
its effect is not visible in the lab. `attunement` was replaced by role
familiarity in player-brain phase B (one value per role; the familiarity states of section 4 come with phase F).

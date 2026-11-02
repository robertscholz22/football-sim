# Player brains: roles, movements and individual coordination

Status: **in migration** (section 16): phases 0, A, B and C done 2026-10-31. First draft 2026-10-30 (as
`possession-movement.md`), revised the same day after two reviews (section 2).

It replaces:
- the team brain (`ai/team_brain.rs` until phase A) as a decision maker, on both sides of the ball;
- the in-possession half of `ai/positioning.rs` (`attack`, `score_attack`, `offer_weights`, the candidate points);
- later, the out-of-possession half (`defend`, `score_defend`).

Unchanged:
- ball physics, the kick solver, duels and locomotion;
- the on-ball valuation (p·V − (1−p)·C, the pass race, the control model);
- the catalog and its sources, the benches and the real-data scripts.

The team brain's geometric functions (blocks, opponent units, offside line, trigger detection) stay as pure functions.
What changes is who evaluates them, and on what inputs.

## 1. Why

The last rounds kept fixing in-possession movement with one more term or weight. The causes are structural.

**Players choose a position, not a movement.** Every 0.25–0.6 s each player scores about 30 candidate points:
- the shape point and his own position;
- three rings of eight points around the shape point;
- four movement-type points (ATT-16: in front, in between, in to out, out to in);
- one run-in-behind point;
- the point he kept from last time.

He then heads for the best one at an urgency proportional to distance (full speed at 20 m). A run in behind, a check
or a decoy run has a trigger, a path, a timing and an end, and none of that fits a point. The run-in-behind point
lies 12 m beyond the line and pays the same distance-from-shape penalty as any other point (≈ −5 against a +0.5
bonus), so it never wins:

| | engine | real |
|---|---|---|
| runs in behind started per striker-match | 0.0 | Number 9s "offer in behind" 26.5 per 90 min [FIFA-N9-WC22] |
| receptions in the attacking half beyond the deepest defender, per team-match | 1.0 | 5.2 raw, 7.6 at the engine's 84 min in play (StatsBomb 360) |
| striker's distance short of the line after 10 s of possession: target / position | 3.0 / 5.8–6.0 m | (most advanced attacker 1.0–1.5 m) |
| off-ball speed in live play | 3.5 m/s | ≈ 2–2.5 m/s |

Targets re-picked several times a second and chased first-order give both the lag and the running.

**Each coaching sentence became an additive score term.** `score_attack` sums about 13 terms. The config has 321
fields, 131 of them tied to catalog rules and 51 to ATT rules. The sum has no meaning of its own: the weights are
the behaviour, and every term competes with every other one. Three cases from the last rounds:
- The lane bonus on the line was unweighted, so pinning beat every ATT-26 cue.
- `commit_cost` (0.3 per metre) outweighs the slope of the shape cost within ≈ 5 m. Nothing pulls a player back
  inside that dead zone.
- The run bonus can never beat the shape cost.

Each fix moved the balance somewhere else.

**Off-ball and on-ball use different currencies.** The carrier chooses by expected value: success probability from
a race along the real trajectory, times the possession value of the outcome, minus the cost of losing the ball there.
The on-ball rounds converged once that pricing matched what the engine does. Off-ball players use stand-ins instead
(lane openness, a between-the-lines bonus, a pin bonus), so they do not move to create the options the carrier will
actually value. That is also why ATT-26 had to hand-code runner / hold-up / link profiles. In a value-based model a
fast striker simply wins the race behind, and a good first touch simply survives a pressed reception.

**Coordination is central where it exists and missing everywhere else.** Out of possession the team brain assigns
pressure / cover / balance (DEF-01…03) from exact times to the ball. That makes a scratch side defend exactly like a
drilled unit. In possession nothing coordinates: each player reacts to team-mates' current positions, which lag
their intentions by metres, and complementary movement is faked with cues. In real football, coordination is
individual. Each player reads the situation and his team-mates, and the team coordinates as well as its players
understand the concept and each other.

## 2. Decisions from the reviews (2026-10-30)

1. **Individual brains.** Each player decides for himself, in and out of possession, and no team brain decides
   anything. The team only holds what the manager sets (formation, roles, instructions) and what the club has built
   (practice, familiarity).
2. **Look-ahead by skill.** Anticipation and match reading decide how far ahead a player thinks: the carrier's next
   pass, the third man, the opponents' response.
3. **Roles from the formation.** The formation decides which role each player fills. A full-back in a 5-3-2 / 5-4-1
   plays a very different role from one in a 4-4-2 / 4-5-1. The role and the instructions together define his ideal
   behaviour and position (run deep behind the winger, play defensively, move inside in possession).
4. **Reading from stats, not noise.** How well a player reads his team-mates depends on three things:
   - his tactical understanding;
   - how much they have played together;
   - how practised the team is in the system and the tactical concept.

Second review:

5. **A minimal attribute palette** ([attributes.md](attributes.md)). *Intelligence* covers tactical understanding and
   how quickly a player learns new team-mates and formations. *Instinct* covers anticipation and choosing the right
   decision. Match reading belongs to intelligence.
6. **Calls** ("man on", "leave it", "step up") come in M6. Making and receiving them depends on how well adjusted the
   players are to each other. A player busy following new instructions, or who doesn't know his team-mates yet, has
   less capacity to coach them. Confidence boosts it.
7. **Familiarity is tested by levels**, e.g. two equally good teams, one settled and one just assembled. That happens
   a lot in real football.
8. **Roles** come from all the formations in use, reduced to the minimal set of positions and interpretations
   (section 5).
9. **Defence first, then attack.**

## 3. Principles

1. A player chooses a **movement**, not a point.
2. A movement is **valued in the on-ball currency** (section 8).
3. Each player has his own **brain**: perception → beliefs → look-ahead → choice → commitment.
4. Coordination comes from a **shared concept** and from **reading** team-mates, not from a central assignment. The
   concept is the role duties and conventions that every player has learned.
5. A player's **role** comes from the formation. The role plus the **instructions** define his duties, his reference
   positions and the movements he prefers.
6. **Errors are wrong beliefs with a cause**: he has not read it yet, he expected the concept, he misjudged a
   team-mate. Their size comes from stats. A team of perfect players coordinates exactly like the optimum: all
   relevant stats at 100, full familiarity, full practice.
7. **Choices are made at triggers and held** until the movement ends, not re-scored every few tenths of a second.
8. **Sources define structure**: the repertoire, triggers, timing, role duties and conventions. Numbers come from the
   value model. The few tuning parameters have a physical meaning. Each source situation becomes a scenario test.

## 4. The player's brain

Each player keeps the following parts, updated on his own clock:

| part | content | from |
|---|---|---|
| perception | the ball; players' positions and velocities (exact, except opponents outside his field of view since M6 scanning: where his last scan saw them) | world state, his reaction time |
| beliefs | his reading of the situation (carrier open or closed, the line, the units, pressure); of each team-mate's current movement and claim; of the opponents' likely response | perception + reading model (section 7) |
| role | duties, reference positions, movement preferences | formation + instructions (section 5) |
| horizon | how many actions ahead and how far in time he looks | instinct, intelligence (section 8.4) |
| plan | his current movement, its phase and its offer window, which team-mates can read | his choice (section 8) |

**World cache.** Computed once per tick from the true state; it is physics and geometry, not decisions:
- the predicted ball trajectory;
- each player's `time_to_reach` for the points that brains ask for;
- the offside lines and opponent units;
- the pressure on the carrier.

Brains read the cache through their beliefs. For example, a player knows the true time a team-mate needs to a point
only as far as he knows that team-mate (section 7). The cache is also what keeps 22 brains affordable (section 12).

**In code (phase A).**
- `ai::world::World` is the cache. It computes offside lines, opponent units and the pressure on the carrier at
  once, and the queries brains ask lazily: time to a point, time to meet a moving point, time to contest a high
  ball, intercepts, receptions, lane openness. Each query is evaluated once per update, however many brains ask it.
- `ai::brain::Beliefs` on each `Player` holds his reading: phase, blocks, every team-mate's duty, pressing, the
  loose-ball claim. `ai::brain::update` evaluates it per player.
- `MatchState::team_view(side)` is what observers (stats, viewer, benches, tests) read. Since phase F it is
  `brain::view`: the first player's beliefs with every duty as its player claims it for himself, the first defender
  who claims the role and the loose-ball chaser who claims the ball. With exact perception and team-mates who know
  each other all players of a side agree, and the view equals any player's beliefs (`tests/brains.rs`).
- The other parts of the brain stay on `Player`: its two roles (`role` in possession, `out_role` out of possession,
  from the slot since phase B), plan (`intent`), and horizon (phase D).

**Where the team brain's jobs go:**

| today (`Brain` field) | in this design |
|---|---|
| `phase`, `phase_since` (EFI phases, BLK-01) | observer: stats and viewer only. Players read what matters to them themselves (counter-press or recover, counter or keep the ball) |
| `def_block`, `att_block` (DEF-05/06, BLK-02…05, ATT-11) | each player evaluates the same block functions on his own beliefs: the ball and the line as he reads them, the instructions as he understands them. The result is his role's reference. With perfect beliefs this is today's block |
| `pressured` (BLK-02: step up or drop) | the line-leading duty (section 5) reads it and leads the line; the others follow him as they read him |
| `def_roles` First / Second / Third, `first`, `first_on` (DEF-01…03) | claims: each defender compares his own time to the ball with what he believes of his team-mates' and takes the duty he has the best claim to (section 8.3) |
| `def_roles` Mark (DEF-04, IND-04) | role zone: he marks the opponents in his zone; the handover works by reading the neighbour |
| `press`, `press_rules`, `trigger` (PRS-01…07) | the first defender perceives the trigger and presses; the others join as they read him or the trigger |
| `counter_*`, `trn01_score` (TRN-01…03, TR-01, TR-04) | each player evaluates the criteria from his own perception. Seeing team-mates counter-press or drop shifts his choice: a group decision made by individuals. Instructions set the prior |
| `att_roles` (Carrier / Receiver / Support / RestDefence, TR-05) | role duties and claims. Rest defence by duty and by how many team-mates he believes stay behind |
| `options` (ATT-09) | each player counts the carrier's options as he believes them |
| `runs_on` (ATT-18) | each player's reading of the carrier (open ball), sharpened by his look-ahead |
| `chaser`, `chase_point`, `challenge` (ATT-22, PRS-02, IND-07) | claims on the loose ball |
| `offside_line`, `opp_units`, `ball_ref` | world cache |

## 5. Roles from the formation

The research behind this section is in [tactics/formations-research.md](tactics/formations-research.md): formation
usage 2018–2026 (men and women), 30 position interpretations with quotes, and the role set below.

A role is what a slot asks of its player. It has four parts:
- **Reference positions** for each phase (in possession by the ball's third, out of possession, rest defence), set
  relative to his unit. These are today's slot `in_pos` / `out_pos`, extended.
- **Duties.** Each duty is a claim he holds or competes for:
  - width on his side, depth;
  - rest defence, covering a team-mate who advances;
  - first defender in his zone, a marking zone, licence to step out of the line;
  - leading the line.
- **Movement priors**: which movements of the repertoire the role uses, with a prior on each.
- **Relations**: the team-mates his duties depend on, e.g. the winger ahead of a full-back, the striker partner, the
  centre-back who covers him. He reads these team-mates first (section 7).

### 5.1 Two roles per slot

UEFA and FIFA describe almost every team with two shapes: "1-4-3-3, defending with 1-4-4-2" [UEFA-UCL24]. The shape
changes because players change duties between the phases: the #10 becomes a second striker, the full-back a
midfielder, the #6 a centre-back. So:
- **A formation slot names an in-possession role and an out-of-possession role.** By default they are the same. A
  4-2-3-1 #10 is AM in possession and ST2 out of possession when the team defends as a 4-4-2.
- **Temporary changes of structure are instructions on the role**, not roles: invert, stay low, overlap, drop into
  the back line, step out. They move the slot's in-possession reference and change its rest-defence membership.
  Every source describes them as something a player *does* in a phase: Chelsea's "Cucurella, inverted high ahead of
  Enzo Fernández as an additional attacking midfielder in possession. When they lost the ball, Fernández recovered
  to the left full-back position" [FIFA-IFB-CWC25].
- **Team-level shapes are instructions, not roles**: the in-possession structure (3-2-5, 2-3-5, 3-2-2-3), the block
  (4-4-2, 4-5-1, 5-4-1, 5-3-2), the rest-defence structure (2+3, 3+2, 3+1, 4+1) and the back four ↔ back five
  switch. Each is a combination of role instructions.
- **Who suits an instruction comes from attributes**: "player profile matters, with inverted full-backs often being
  elite counter-pressers" [FIFA-IFB-CWC25]. The fit shows in play (section 8 prices the movements on each player's
  physics) and in the manager's choice, not in a separate role.

**What the engine must play.** Usage figures say which shapes to support. They are not calibration targets; real
teams differ by style.

| competition | most used | back three or five |
|---|---|---|
| WC 2022, 126 line-ups [FIFA-WC22-PMR, own count] | 4-1-2-3 43 %, 4-2-3-1 14 %, 4-4-2 11 %, 3-4-3 11 %, 5-3-2 6 % | 27 % of line-ups; 18 of 32 teams at least once |
| Women's WC 2023, 128 line-ups [FIFA-WWC23-PMR, own count] | 4-1-2-3 27 %, 4-2-3-1 23 %, 4-4-2 20 % | 23 % of line-ups |
| EURO 2024, shapes by phase [UEFA-TR24] | attacking 3-2-2-3 (33 uses), 4-3-3 (27); defending 4-4-2 (21 of 24 teams), 5-4-1 and 5-3-2 (16 each) | 52 % of attacking-phase uses; 8 of 24 teams *started* with one |
| UCL 2023/24, 32 teams [UEFA-UCL24, own count] | 4-3-3 13, 4-2-3-1 11, 4-4-2 3 | 4 teams |

Half of all teams use a back three at some stage, and the attacking shape is often a back three built from a back
four. Diamonds and 4-2-2-2 appear in 1–5 % of uses.

### 5.2 The role set (1 goalkeeper + 13 outfield roles)

| role | slot | what sets it apart | relations | sources |
|---|---|---|---|---|
| **GK** | goalkeeper | defends the goal and the space behind the line; extra build-up player | the line (sweeping height) | FFA-NC, FFA-433 |
| **CB** | centre-back in a pair | two share the centre; one steps, one covers; marks or covers by the number of strikers; leads the line | partner CB, FB | FFA-NC, BDFL-PETER09, UEFA-UCL24 |
| **CBC** | central CB of a three or five | "spare man and chief organiser" [UEFA-UCL25]; covers behind two stepping CBWs, so he can follow a dropping 9 | both CBWs | UEFA-UCL25, FIFA-GI-CHE, BDFL-WM14 |
| **CBW** | outside CB of a three or five | half-space lane; licence to step out ("free to step up" [UEFA-TR24]); covers behind the wing-back; takes the wide lane in possession ("besetzen in Ballbesitz die Außenspuren" [DFB-AK-IV]) | CBC, WB | UEFA-TR24, UEFA-UCL24/25, DFB-AK-IV |
| **FB** | full-back in a back four | shares the wing with a W or WM ahead; width set relative to him; narrow in a block | W or WM ahead, CB | FIFA-IFB-CWC25, FFA-NC, FIFA-TSG |
| **WB** | wing-back in a back five | only width on his side; in the front five in possession; drops into the last line or jumps to the wide player because the CBW covers ("insurance of an extra player not just behind you but inside you" [UEFA-UCL25]) | CBW, WM or AM | UEFA-TR20, FIFA-GI-CHE, UEFA-UCL25 |
| **DM** | single pivot | alone in front of the line; protects the back four; the #8s guard his sides; the "+1" of a 2+3 rest defence | CM8s, CBs | FIFA-WC26, UEFA-TR24, FFA-433 |
| **CMP** | one of a central pair (double pivot or flat two) | "One of the two should always join in to support the attack while the other one stays behind the ball" [FFA-NC]; "if either centre-midfielder came out to press, the wide midfielder needed to come in narrow" [FIFA-WC26] | partner CMP, WM | FFA-NC, BDFL-PETER09, UEFA-TR20 |
| **CM8** | #8 beside or ahead of a single pivot | half-space receiver and runner; first presser out of the midfield line; covers the width in front of a back five | DM, FB or WB, W | UEFA-TR24, FIFA-WC26, FFA-433 |
| **AM** | attacking midfielder between the lines | receives in the pockets; lane parameter: centre or half-space ("two No10s in an attacking 3-4-2-1" [UEFA-TR24]) | CF, ST2 | FIFA-WC26, FFA-NC, UEFA-TR24 |
| **WM** | wide player of a midfield line | wide lane of the midfield line; tucks in when the CMP or CM8 steps out; tracks the full-back | CMP or CM8, FB | FIFA-WC26, UEFA-UCL25, BDFL-PETER09 |
| **W** | winger or wide forward | front line; pins the full-back high and wide; wide or inside relative to the full-back ("you go outside when he goes inside" [UEFA-UCL25]); presses from the front | FB or WB behind | FFA-NC, FIFA-WC26, UEFA-TR24 |
| **CF** | lone centre-forward | must both pin the line and connect play; pins or comes short by situation and profile (ATT-26); decides when the press starts | AM, Ws | FFA-NC, FIFA-N9-WC22, FIFA-WC22-OV |
| **ST2** | striker in a pair | one offers short, one holds the depth; out of possession the ball-side one presses the CB, the other covers | partner ST2 | UEFA-UCL25, DFB-TE-2ST, DFB-RP16 |

Every role keeps the attacking-play rules of today's catalog. The role decides which duties he holds and which
movements his priors favour; the situation and his attributes decide what he does (no blanket exemptions, ATT-26).

**Why these splits.** The criterion is the one used for attributes: split only where the duties really differ.
- CB / CBC / CBW: lane, licence to step out, who covers whom, and width in possession all differ. A back five lets a
  centre-back follow a dropping striker "ohne hinter sich Räume zu öffnen" (without opening space behind him)
  [BDFL-WM14].
- FB / WB: shared width vs sole width, and the covering CBW inside the wing-back.
- DM / CMP / CM8: one player alone in the centre vs a pair who cover each other vs a #8 beside a pivot.
- WM / W: midfield line vs front line, and different covering relations.
- CF / ST2: a pair splits the jobs a lone 9 must do by situation.

**Merged.** A double pivot and a flat two are both CMP (a 4-2-3-1 defends as a 4-4-2). The central #10, the two
inside #10s and the diamond tip are AM with a lane. The diamond's sides are CM8, its base DM. Inverted, low and
overlapping full-backs are FB with instructions; a dropping #6 is a DM or CMP instruction; an inside winger is W
with the wide/inside instruction; a false 9 is CF behaviour. The libero and the "half-back" are covered by CBC and
the DM drop instruction.

### 5.3 Formations as role pairs

In-possession role → out-of-possession role, named only where they differ:
- **4-3-3:** GK, CB×2, FB×2, DM, CM8×2, W×2, CF. As a 4-5-1 block, W → WM; as a 4-4-2, one CM8 → ST2 and W → WM.
  Building in a 3-2-2-3: one FB stays low and the other inverts, or the DM drops.
- **4-2-3-1:** CB×2, FB×2, CMP×2, AM (centre), W×2, CF. As a 4-4-2, AM → ST2, CF → ST2, W → WM.
- **4-4-2:** CB×2, FB×2, CMP×2, WM×2, ST2×2.
- **Diamond 4-4-2:** CB×2, FB×2, DM, CM8×2, AM (centre), ST2×2.
- **3-4-2-1:** CBW×2, CBC, WB×2, CMP×2, AM×2 (half-spaces), CF. As a 5-4-1, AM → WM; as a 5-2-3, AM → W.
- **3-4-3:** CBW×2, CBC, WB×2, CMP×2, W×2, CF.
- **3-5-2 / 5-3-2:** CBW×2, CBC, WB×2, DM (or a CMP pair), CM8×2, ST2×2.
- **4-4-2 dropping into a back five:** FB → WB out of possession.

### 5.4 Instructions (layer 2)

Instructions change a role's duties, references and priors. Each one is per player and resolvable:

| role | instructions |
|---|---|
| GK | build-up involvement; sweeping height |
| CB | step into midfield ("Akanji stepped into midfield and formed a double pivot with Rodri" [UEFA-UCL24]); follow or hand over a dropping 9 |
| CBC | carry into midfield; mark the 9 or stay spare |
| CBW | overlap / underlap / stay home; step-out aggressiveness |
| FB | overlap / underlap; **invert** (beside the pivot); **stay low** (makes the back three: "the right full-back tucked in alongside his centre-backs, so they maintained a 3+2" [FIFA-IFB-CWC25]); high and wide; drop into a back five out of possession |
| WB | invert into an #8 or #10 ("left wing-back Michel Aebischer excelled by coming inside into the No10 position" [UEFA-TR24]); stay deep |
| DM, CMP | **drop into the back line**: France "switched from building in a back four to a back three. By dropping their deep-lying central midfielder, Adrien Rabiot, between the two centre-backs" [FIFA-WC26-B3]; mark the 10 or zone; hold / advance (CMP, per pair) |
| CM8 | press trigger; run beyond or stay; go wide when the wing-back inverts |
| AM | lane; free role; drop to build; run in behind |
| WM | tuck in or stay wide; track the runner or pass him on |
| W | wide / inside; press outside-in; stay high for the counter |
| CF | pin / come short weighting (today's `come_short`); target play; press trigger |
| ST2 | who offers and who holds the depth (per pair); press the CB or screen the pivot |

Plus the team-level shapes of 5.1. The existing instructions ("come short", "make runs in behind", "stay wide",
"stay back") stay. The M5 presets (false 9, inverted full-back, target man) become named combinations of these.

### 5.5 Data and catalog

- **Roles.** The old `Role` enum (GK, CB, FB, DM, CM, AM, W, ST) became the 14 roles (phase B). Role definitions live in
  `data/roles/*.ron`; formation files give each slot its two roles and its reference positions per phase.
- **Catalog.** Each role becomes a catalog entry with the quotes behind its duties, relations and instructions
  (written in phase B from the research notes). A role duty is layer-1 behaviour like any other rule.
- **Role familiarity.** The former `attunement` became familiarity per role (phase B), growing with experience in that role and
  faster with intelligence. A slot uses the familiarity of the role active in the current phase, so a #10 who has
  never pressed as a second striker is weaker out of possession.

**Gaps.** Federation coaching material covers the 4-3-3 family and back-four play well, but says little about
wing-backs, about central vs outside centre-backs, diamond sides or inside #10s. Those duties rest on the UEFA and
FIFA technical reports (analyses of elite teams by their technical observers). No source gives time spent in each
shape, or how often teams use each instruction.

## 6. Movements

A movement is a small state machine. It has:
- a **kind**;
- a **path**, relative to moving references: the role's reference position, the ball, the offside line, a named
  team-mate or opponent;
- a **speed profile**: walk / jog / sprint, or "arrive at time T";
- **start conditions** and **end conditions**;
- its **offer**: where, and from when to when, he can take a pass. This is what the value model prices.

The vocabulary is FIFA's "movement to receive" (in front, in between, in to out, out to in, in behind) [FIFA-FL],
plus what the sources describe around it.

| movement | path and timing | starts when | ends when | sources (catalog) |
|---|---|---|---|---|
| **hold** | stay at the role's reference; walk or jog back only when it moves out of a band | default | any other movement is chosen | ATT-11, TR-05 |
| **show** (in front) | to a point at support distance with an open lane, arriving by the carrier's next decision | the carrier has or is about to have the ball and lacks a short option | the ball is played, or the carrier turns away | ATT-08, ATT-09, USS-D, FFA-GTC |
| **check** (from the line to feet) | from the line 3–8 m towards the ball, ending with a burst away from the marker; may turn into a spin in behind | the carrier is closed (pressed, or facing his own goal) | the ball is played, or the carrier opens up (→ spin) | ATT-26, FA-FWD, FIFA-HED, FIFA-GI-BIB, DFB-RP16 |
| **drop** (in between) | into the space between the opponents' midfield and defensive units, away from the nearest opponent | space between the units and a carrier who can play through | the ball is played, or the space closes | ATT-15, BDFL-HALE, FIFA-N9-WC22, FIFA-U17W-CF |
| **pin** (hold the last line) | onside on the deepest defender, moving with the line at its speed | default for a depth duty when nothing better is on | another movement is chosen | ATT-11, ATT-26, DFB-SCHOEN, FIFA-U20-CF |
| **run in behind** | wait onside, then sprint timed to cross the line as the pass is struck; arced to stay onside until then | open ball: the carrier faces forward with time and can play that pass | the pass comes to him (→ receive) or goes elsewhere; the carrier closes; the run is offside or the space is gone (→ recover onside) | ATT-18, FA-FWD, FIFA-HED, FIFA-N9-WC22 (timing, arced runs), FIFA-GI-BIB |
| **in to out / out to in** | lateral run between a wide and an inside channel | a channel is free (ATT-12), or the full-back is drawn | as for show | ATT-12, FIFA-FL, FIFA-JOHNSON |
| **overlap / underlap** | run outside or inside the team-mate on the ball side, beyond him | the team-mate holds the ball or comes inside; the role or an instruction allows it | the ball is played, or the team-mate turns back | ATT-20 (FIFA-ES Zamrat) |
| **invert** | in possession, a full-back or wing-back moves from the wide reference into the inside channel beside or ahead of the pivot; a team-mate (W, CM8, CBW) takes the wide lane | instruction (5.4), from the build-up on | possession lost: back to his slot, or the pivot covers it ("Fernández recovered to the left full-back position") | FIFA-IFB-CWC25, DFB-AK-MCI, UEFA-TR24, UEFA-UCL24 |
| **drop into the back line** | the pivot drops between or beside the centre-backs, who split; the full-backs push on | instruction, or the first press line outnumbers the centre-backs | the first line is beaten, or possession lost | BU-03, BU-04, FIFA-WC26-B3, FIFA-IFB-CWC25 |
| **step into midfield** | a centre-back steps out of the back line into the midfield line, with or without the ball | instruction; the space in front of him is free | possession lost, or the ball goes past him | UEFA-UCL24 |
| **decoy** | a run whose value is the defender it drags, for a named team-mate | a team-mate's offer gains from that defender moving | the ball is played, or the team-mate's offer ends | FIFA-N9-WC22, BDFL-HALE |
| **receive / meet** | unchanged: the ATT-24 movement onto the ball's line | a pass is set up for him or struck to him | control | ATT-24 |
| **rest defence** | hold behind the ball by the TR-05 gap | rest-defence duty | the duty ends | TR-05 |

**Out of possession (phase C)**, in the same framework:
- press / delay (DEF-01, IND-01…03, PRS-03), cover and balance (DEF-02);
- mark (DEF-04, IND-04), track a runner (IND-04; FA-EFL "space in behind");
- lead or follow the line (BLK-02), recover (IND-06), counter-press (TRN-01…03).

These are valued by denial, the mirror of the offer value: how much of the most dangerous offers they remove.

**Added in M4:** box runs onto the near post, far post, penalty spot and edge of the box (FT-03), offered while the
ball is wide in the final third.

**Later:**
- third-man runs (ATT-20, M5);
- counter running lanes (TR-03, M5).

Every movement carries its rule IDs. The viewer shows the movement, its phase (waiting, running, offering, ending),
its value and its rules. It also shows what the player believes his team-mates are doing.

## 7. Reading team-mates

This section models how well players coordinate. Everything in it is a deterministic function of stats and
situation.

**Understanding.** For player *i* reading team-mate *j*, u_ij ∈ 0…1 is multiplicative, like the existing reliability:

`u_ij = INT_i · f(practice) · f(partners_ij) · f(1 − load_i)`

The terms:
- INT_i: the intelligence of *i*, his tactical understanding ([attributes.md](attributes.md)).
- `practice`: how practised the team is in the formation and its roles, and in the tactical concept (the
  instructions). This is team data; in the manager game it comes from training.
- `partners_ij`: how much *i* and *j* have played together. This is pair data; in the manager game it comes from
  minutes played together.
- `load_i`: his instruction load. New instructions or a new role take attention until they become routine.
- f(x) = floor + (1 − floor)·x, as for the existing state factors, so that no single factor zeroes understanding.

**Expectation.** Until *i* has read *j*'s current movement, he believes *j* is doing what the concept prescribes for
*j* in this situation: *j*'s role duties and the conventions, as *i* computes them.
- If *j* follows the concept, *i* is right at once. Drilled teams coordinate before anyone has seen anything.
- If *j* deviates, *i* stays wrong until he reads the movement. A deviation can come from *j*'s own reading, his
  egotism, low discipline, or a better idea.

**Reading delay.** *i* reads *j*'s actual movement `d_ij = d_max · (1 − u_ij)` after it becomes legible. Legibility
is geometric: a check and a spin in behind start alike, so a run that starts as a check reads as a check until the
two paths separate.

**Knowing a team-mate.** Claims depend on team-mates' abilities: how quickly he gets to the ball, whether he wins
headers. *i* estimates them through `partners_ij`. A partner he knows well is estimated exactly; a new signing is
estimated as an average player in his role.

**Reading the passer.** The intended receiver anticipates a pass as far as he knows the passer and the team's play:
he starts to take it `d_r = d_r,max · (1 − F_rp)` after it becomes legible (the passer's set-up, or the strike), and
until then keeps his movement. F_rp is u_rp without the intelligence term, `f(practice) · f(partners) · f(1 − load)`:
his reading of play in general is already in his reaction, and with intelligence in it a settled side's receivers
would all be late.
The passer prices this in the pass's race, so a side that does not know each other completes fewer passes and plays
fewer risky ones. This is where the research places the familiarity effect: pairs who know each other pass to each
other more and complete more, and stability predicts goals scored rather than goals conceded
([familiarity-research.md](tactics/familiarity-research.md) section 5). Claims and movements alone barely move the
result (phase F bench, section 16).

**Instructions.** *i* applies the team's instructions as far as he follows and understands them: his discipline
(exists), the practice of the concept, and his role familiarity.

**No noise.** Errors are wrong beliefs with a cause, and the viewer shows them: what *i* believes *j* is doing next
to what *j* does. Randomness stays only where it is now, in each player's own decision deviation (layer 3 of the
choice, temperature 0 for a perfect player). Where u = 1, beliefs equal the truth, so a perfect team coordinates
like the optimum.

**Calls (M6).** A call is a short message that replaces reading for the player who receives it. His belief updates
at once, without `d_ij`. Calls cover:
- pressure behind him: "man on", "time";
- a claim on a loose ball or a header: "mine", "leave it";
- the line: "step up", "drop";
- pressing: "I go", "you go";
- the next action: "switch", "turn".

*Making* a call needs three things:
- the caller reads the situation (on his own beliefs, so a wrong read gives a wrong call);
- he has spare capacity;
- the call is worth making: the value of the receiver acting on it.

Capacity grows with leadership and confidence. It shrinks with instruction load and with how little he knows the
team-mates involved (partner familiarity, system practice). *Receiving* works the same way: the receiver acts on a
call as far as he knows the caller and has capacity himself.

Capacity refills a call budget, and whenever the budget allows, he makes the most valuable call. No dice are rolled.
Calls should mean fewer double presses, earlier line steps and safer first touches under pressure. A settled side
with leaders gains most.

**Reading opponents.** *i* predicts the opponents' responses (the line drops, a marker follows) with the defending
rules, as far as his look-ahead and instinct reach (section 8.4). Scouting knowledge of a particular opponent
comes later.

## 8. Choosing a movement

### 8.1 Offer value

A movement is worth the best option it offers the carrier during its window. For each sample point *x(t)* of the
offer:

`q(t) = P_play · p_arrive · (1 − L_pressed) · V(x) − (1 − p_arrive) · C(loss point)`

The terms:
- `V` and `C` are the on-ball possession value and turnover cost, including the ATT-17 line-break value and xG in
  shooting range. Off-ball and on-ball values are therefore in the same unit, and instructions and personality
  biases compare across both.
- `p_arrive` is a cheap surrogate of `evaluate_pass`:
  - a nominal ground or lofted pass from the carrier;
  - opponents' `time_to_reach` to points along the lane;
  - the receiver's own `time_to_reach` to *x* against the nearest opponent's, through the race sigma.

  The receiver's attributes enter here: pace and acceleration through the reach model.
- `L_pressed` is the ATT-25 loss for a pressed receiver, through `pressed_at` and the control model. First touch and
  composure enter here.
- `P_play` is whether this carrier can play that ball at all. It covers:
  - his vision cone and passing range;
  - whether he is open (facing forward with time) or closed;
  - his technique for a through ball.

  This is where the FA's cue *emerges*: head up, run in behind; head down or under pressure, "drop-off and provide an
  option to play to feet" [FA-FWD]. A closed carrier cannot play the ball in behind, so the run is worth nothing and
  the check is worth a lot.

Everything here is computed on the player's beliefs. A striker who misreads the carrier as open values a run that
is not on.

Player type therefore comes out of the physics and the duel models instead of a hand-made profile softmax:
- a runner's run in behind has a higher `p_arrive`;
- a link player's check has a lower `L_pressed`;
- a target man's pin has a higher control probability in the air.

### 8.2 Space value (pins and decoys)

A pin or a decoy is worth the gain in a team-mate's best offer when the defender it occupies responds. The response
comes from the defending side's own rules, predicted cheaply:
- the zonal marker follows within his zone and releases at its margin (DEF-04, IND-04);
- the back line drops while the carrier is open and steps up when he is pressed (BLK-02).

The sources describe exactly this:
- the striker's run "zieht … die Kette weg" and opens the room between the lines for the 10 [BDFL-HALE];
- team-mates "make decoy attacking runs in behind to drag defenders deep", which gives the 9 "an extra yard on the
  centre-backs" [FIFA-N9-WC22];
- when the 9 drops deep, "the wide players should run into the space left behind" [FIFA-JOHNSON];
- the back line drops when the carrier has time and a striker runs in behind, and steps up when the carrier is under
  pressure and the striker comes short [FA-EFL, "space in behind"].

How far ahead a player sees these responses depends on his look-ahead (section 8.4).

### 8.3 Marginal value and claims

A player values his offer by what it adds to the carrier's set of offers *as he believes it*: the expected best
option with his offer, minus without it. Examples:
- a second run into the same space adds little;
- an offer whose lane the same defender already covers adds little (ATT-08: "one defender cannot restrict both");
- the only short option when the carrier is pressed adds a lot.

Complementarity thus follows from the value, and the scenario tests check it:
- ATT-10: "avoid offering the same supporting movements" [FIFA-TCP];
- if one striker offers, the other takes the position in depth [DFB-RP16];
- the 10 does not copy the striker's run in depth [BDFL-HALE].

**Duties are claims.** A few things must be done by someone. Each is a convention of the concept with a source, and a
**claim function** decides who does it:

| duty | claim |
|---|---|
| first defender (DEF-01) | time to the ball |
| rest defence (TR-05) | role duty, then depth behind the ball |
| at least three options (ATT-09 [FFA-GTC]) | value of the best show / check |
| depth (ATT-11 "so tief wie möglich" [DFB-FLICK], DFB-RP16) | role duty, then distance to the line |
| width and centre (ATT-11 / 12 "high and wide" [FFA-NC]; ATT-13 [FIFA-ES]) | role duty, then distance to the channel; only if marginal value does not deliver them in the scenario tests |

Every player computes the claims himself: his own exactly, his team-mates' as he believes them. He takes a duty when
his claim beats every claim he believes his team-mates have. With perfect beliefs, exactly one player takes each
duty. With imperfect beliefs, two go or nobody does, until reading catches up. That is the realistic failure, and its
frequency comes from stats.

### 8.4 Look-ahead

| level | what he considers |
|---|---|
| 0 | offers to the current carrier as he is now; reacts to what has happened |
| 1 | anticipates the carrier's next action: which pass he is likely to play (a cheap form of the on-ball ranking) and when he turns open. Values offers to the next carrier too (a third-man run starts as the pass to the 9 is struck). Anticipates the opponents' first response (the line drops, the marker follows) |
| 2 | two actions ahead: combinations (ATT-20) |

Implemented (watch round, 2026-11-02): level 1 as the carrier's most likely pass (softmax over keeping and the
team-mates' offers, `offer::next_passer`), whose receiver is the next carrier; a movement's value adds what its offer
adds to his options, times that pass's probability, and its candidate points include those offered to him. The
opponents' first response (the marker follows, `offer::responded`) was there before. Level 2 is not implemented.

The horizon is continuous: the weight a player gives to level-1 and level-2 values, and how many seconds ahead he
projects. Two ratings set it ([attributes.md](attributes.md)):
- **instinct** (anticipation): how early and how accurately he projects physical events. It also sets reaction time
  and the touches on passing balls.
- **intelligence** (match reading): how many decisions ahead he thinks.

Timing follows from both:
- a short horizon gives the classic late start ("he only ran once the pass was played");
- a wrong projection gives the early start (offside, the run is wasted).

### 8.5 Effort

A cost per metre, by speed class, scaled by work rate. A movement must pay for itself. That is what keeps players
still or walking when nothing is on, which matters for the off-ball speed target.

## 9. When decisions happen

A player re-chooses at **triggers**, each perceived at his reaction time after the event:
- possession won or lost;
- a pass struck (ATT-22 "certain passes should trigger specific movements" [FIFA-ES]);
- a team-mate's control;
- the carrier turning open or closed;
- a pass set up for him (ATT-24);
- his own movement's end condition;
- reading a team-mate's movement that changes a claim or a marginal value;
- the offside line or his reference position moving out of a band.

Between triggers there is only a slow periodic check (about 1–2 s instead of 0.25–0.6 s). During a movement he
*follows its path*: a pin tracks the line, a run bends along it, a check bursts at its end. That is path tracking,
not a new choice. Switching to another movement before the end needs a margin (hysteresis). Fewer, event-driven
decisions are also how the model pays for its more expensive valuation (section 12).

## 10. Moving

- **Speed by intention.** The first-order urgency (distance / 20 m) is replaced by the movement's profile:
  - hold walks or jogs and stops;
  - show and check jog, then burst;
  - a run waits, then sprints;
  - pin matches the line's velocity. This feed-forward applies to this movement only; the earlier version for all
    roles raised every player's speed.
- **Arrive at a time.** Where timing matters (show to the carrier's next decision, run to the pass), speed = distance /
  time available, capped by the player's kinematics. This uses `time_to_reach` consistently with the race model.
- **Unchanged:** the ATT-24 receive steering, locomotion physics and the layer-3 drift (now only while holding).

## 11. The three layers

- **Layer 1: the concept.** The repertoire, the valuation, the role duties from the formation and the conventions.
  All sourced.
- **Layer 2: instructions.**
  - The choice of formation, which sets the roles.
  - Role instructions: overlap, stay back, move inside, come short, runs in behind, stay wide.
  - Team instructions: line, press, width, directness, tempo.

  Each is applied as far as the player follows it (discipline) and the concept is practised.
- **Layer 3: the individual player.**
  - Reading (section 7): intelligence, familiarity with team-mates, practice of the system and concept, role
    familiarity, instruction load.
  - Look-ahead (section 8.4): instinct, intelligence.
  - Decision deviation: instinct, composure, confidence, pressure. As now, with temperature 0 for a perfect player.
  - Timing: emerges from reading and look-ahead; focus lapses (composure, energy) since M6 (`deviation::lapse_rate`).
  - Calls (M6, section 7, `reading::call_rate`): leadership, confidence, instruction load.
  - Drift while holding: as now.

## 12. Interfaces and scope

- **On-ball:**
  - The carrier is a brain too. He passes into a run as he reads it, so a late read means a late through ball.
  - The through ball aims at the run's offer as he believes it, instead of 7 m ahead of the team-mate's current
    velocity.
  - His existing one-step valuation becomes his look-ahead level 1.
- **Offside** is not enforced until M7. The carrier treats an offer from an offside position as worth nothing, so an
  early start costs the chance without the law being implemented.
- **Defending** migrates first, in phase C (section 16). Realistic runs need realistic tracking: if nobody follows runners,
  runs become free chances; if everybody always follows, runs are worthless. The scenario tests cover both.
- **Perception:** since M6 (`ai::scan`) a player sees the opponents in his field of view; the others are where his
  last scan put them. The ball and his team-mates he sees exactly, so reading team-mates (section 7) does not depend
  on whether he looked (open).
- **Performance.** Today one match takes ≈ 6.5 s headless (target < 5 s). Four things keep 22 brains within budget:
  - the world cache, computed once per tick;
  - trigger-driven choices (a decision every 1–2 s instead of every 0.4 s on average);
  - brains far from the ball that are holding skip the valuation;
  - each player keeps beliefs in detail only for his relations and the players near the ball.

  That leaves room for ≈ 6–10 movement options × 11 opponents' `time_to_reach` per decision. The prototype measures
  this first.

  Measured in phase A: 22 brains evaluating the team brain's logic over the cache cost +0.17 s per match (6.6 → 6.8 s
  single-threaded, +2.6 %).

## 13. Catalog mapping

| rule | team brain (until phase G) | in this design |
|---|---|---|
| ATT-08 support angle and distance | lane-openness term | the **show** offer, priced by the race; "one defender cannot restrict both" = marginal value |
| ATT-09 three options | team-brain count + support boost | emerges from marginal value (test 7) |
| ATT-10 staggering | collinear / same-line / crowding penalties | emerges from marginal value; scenario test; a duty only if needed |
| ATT-11 width and depth | shape + onside penalty | role reference positions; depth duty; **pin** |
| ATT-12 five channels | channel bonus | **in to out / out to in** values; width duty if needed |
| ATT-13 central occupation | central bonus | centre-lane roles' spots; a duty only if a test shows the centre empty |
| ATT-14 cover shadows | shadow penalty | emerges from the race (a lane in a cover shadow is intercepted); DFB-SCHOEN scenario test |
| ATT-15 between the lines | between bonus | **drop** |
| ATT-16 movement types | extra candidate points | the repertoire itself |
| ATT-17 line breaks | pass bonus | the same function in the offer value |
| ATT-18 runs in behind | team-brain `runs_on` + one candidate + bonus | **run in behind** with trigger and timing |
| ATT-20 combinations (M5) | — | **overlap / underlap**; third-man runs through look-ahead level 1–2 |
| ATT-22 mobility | re-evaluate at pass release | a trigger |
| ATT-24 meet the ball | movement | unchanged |
| ATT-26 pin or offer short | profile softmax + cues | emerges from value: profiles through physics, cues through `P_play` and marginal value; `come_short` stays as a layer-2 bias |
| TR-05 rest defence | team-brain assignment + penalty | duty + **rest defence** movement |
| TR-01 / TR-04 counter or keep | team-brain decision | each player's reading of the criteria, influenced by team-mates' visible choice; run in behind in transition (deeper centre-forwards "accelerate into the space separating them from the opposition's defensive line" [FIFA-N9-WC22]) |
| DEF-01…03 pressure / cover / balance | team-brain assignment by time to the ball | duty claims per player (phase C) |
| DEF-04 / IND-04 marking | team-brain zones | role zone, handover by reading the neighbour (phase C) |
| DEF-05 / 06, BLK-02…05 block | team-brain block | the same functions on each player's beliefs; the line led by the line-leading duty (phase C) |
| PRS-01…07 pressing | team-brain trigger and press flag | the first defender perceives the trigger; the others read him (phase C) |
| TRN-01…03 counter-press or recover | team-brain decision | per player, as TR-04 (phase C) |
| BLK-01 EFI phases | team-brain phase | observer (stats, viewer) |
| FT-03 box occupation | — | **box run** (M4) |

**Removed parameters (18):** `att08_lane_weight`, `att09_boost`, `att10_*` (4), `att12_channel_bonus`,
`att13_central_bonus`, `att14_shadow_deg`, `att14_penalty`, `att15_between_bonus`, `att18_run_depth`, `att26_*` (6).
The candidate rings, `commit_cost`, `settle_radius` and `position_full_speed_dist` stay for defending until phase C;
attacking play stops using them.

**New parameters**, each with a physical meaning and calibrated against the conditional targets below:
- effort cost per metre by speed class, the switch margin and the periodic check interval;
- the reading delay `d_max` and the reading floors;
- the look-ahead weights.

## 14. Data

- **Roles:** the 14 roles of section 5.2 in `data/roles/*.ron`. Formation files give each slot an in-possession and
  an out-of-possession role. Beside today's 4-4-2 and 4-3-3: 4-2-3-1, 3-4-2-1, 3-4-3 and 3-5-2 / 5-3-2 (5.3).
- **Team files:**
  - `practice { formation, concept }`, each 0…1;
  - `partners`: a squad default plus overrides per pair;
  - role familiarity per player (`familiarity` and `role_familiarity` in the squad files since phase B).

  Until the manager game provides them, presets describe sides from settled to just assembled (section 15).
- **Attributes:** the palette in [attributes.md](attributes.md). Role familiarity, partner familiarity, system
  practice and instruction load are states, not ratings.

## 15. Validation

**Scenario tests** in `tests/principles.rs`, one per source situation:
1. Open ball, fast 9, a high line: he starts a run timed to the pass, and the through ball finds him behind
   [FA-FWD; FIFA-HED; FIFA-N9-WC22].
2. Carrier pressed or facing his own goal: the front player offers to feet instead of running [FA-FWD]. (Phase D:
   with the carrier's vision cone, a carrier facing his own goal cannot play forward at all; the test uses a pressed
   carrier, for whom the ball in behind is gone and an offer to feet — on the line or short of it — remains.)
3. One striker shows: the other holds the depth [DFB-RP16].
4. DFB-SCHOEN 4.1: the strikers in the shadow of two stepped-up midfielders go deep while the ball-far midfielder
   offers in the gap [DFB-SCHOEN].
5. The 10 does not copy the striker's run in depth, and offers between the lines when the line is pulled away
   [BDFL-HALE].
6. The 9 drops: a winger runs in behind [FIFA-JOHNSON]. (Moved to M5 with BLK-03: the space left behind opens only
   when the back line shuffles across as a centre-back follows the 9; without that shuffle the other centre-back
   still covers the winger's lane, and the run is rightly worth no more.)
7. Build-up: at least three options; rest defence holds [FFA-GTC; TR-05].
8. Same situation, runner vs link profile: different movements.
9. `forward_runs` +1 and `come_short` +1 change the choice.

**Tests for brains, roles and reading:**

10. A perfect team: exactly one first defender, one depth provider, the rest-defence count, no duplicate runs, the
    same as the optimum. (Since M6 the line leader's call reaches team-mates at their next update, or after their
    reading delay when he has no call in hand. While it is on its way their blocks and claims can disagree, so out of
    possession the test allows rare errors that end within `d_max` plus two brain ticks. The belief-identity test
    leaves out what arrives by call or reading.)
11. Low partner familiarity (a new signing): duplicate runs and double presses or no press occur, and resolve within
    `d_max`. (Since M6 the settled opponents err too, while a line call is on its way; the test requires them to err
    less than a quarter as often as the side with the signing.)
12. Same players, concept practice 1 vs 0.2: shorter reading delays with practice. (Until M4 this test counted
    coordination errors over full matches. With twelve seeds the difference was within noise, 33.8 vs 33.1 error ticks
    per 1000, so the test now checks the delays directly.)
13. Full-back in a 4-4-2 vs wing-back in a 5-3-2: different width, height and rest-defence share.
14. "Move inside", "overlap" and "stay back" change the role's choices.
15. Look-ahead: a player with high intelligence starts the third-man run as the pass to the 9 is struck; a low
    reader starts after the 9's control.

**Benches against situation-conditional real data:**

| measure | bench | real |
|---|---|---|
| pinning by the line's distance from goal | `forward_moves` | `sb_front_pin.py` |
| centre-forward reception classes by line and passer pressure; per-player spread | `forward_moves` | `sb_fwd_recv.py` |
| receptions beyond the line; through balls (volume, completion) | `forward_moves` | `sb_behind.py`: 7.6 per team-match at engine time; CF 1.7, W 2.0; through balls 2.0, 35 % completed |
| runs in behind per striker | `forward_moves` | FIFA 26.5 offers in behind per 90 min |
| most advanced attacker's gap to the line; box mates | `final_third` | 1.0–1.5 m; 0.7–2.4 by distance |
| off-ball speed and walking share | `movement` | ≈ 2–2.5 m/s |
| average positions and width by role (full-back vs wing-back) | new | StatsBomb 360 by position and formation |
| coordination errors per match (double press, no press, duplicate runs) by familiarity | new | qualitative; no published counts known |

**Familiarity bench** (`familiarity`, new). Two teams with the same ratings (one squad, mirrored), at several
levels. The targets are the settled side's edge per match, from
[familiarity-research.md](tactics/familiarity-research.md) section 3:

| level | role familiarity | partners | practice | settled side's goal difference per match: best (range) |
|---|---|---|---|---|
| settled | 0.9 | 0.9 | 0.9 | — |
| new manager, new system | 0.9 | 0.9 | 0.2 | +0.04 (0 to +0.10) |
| several signings | 0.8 | 0.5 | 0.6 | +0.08 (+0.02 to +0.17) |
| just assembled | 0.6 | 0.1 | 0.2 | +0.25 (+0.10 to +0.50) |

The evidence behind them:
- **Several signings and just assembled.** These come from league studies that control for team quality:
  - Franck et al. 2011 [FRANCK11] and Gerrard & Lockett 2018 [GERRARD18], on average prior appearances for the club;
  - van Vugt et al. 2008 [VANVUGT08], on squad stability.
  
  They are pulled down because appearances also measure whom the coach rates. Studies that compare teams with
  themselves, or national teams, find no effect: the reanalysed Mukherjee et al. 2019 [MUKHERJEE19] and Le Coz et
  al. 2025 [LECOZ25]. "Just assembled" lies at the edge of the league data and is an extrapolation.
- **New manager, new system.** Coach changes that handle regression to the mean put the net effect near zero:
  [HEUER11], [BRYSON21], [VANOURS16]. One formation change costs 0.02–0.07 goals in the next match [TAMURA15]. That
  bounds the cost of learning a system at about 0.1 goals per match.

Each level plays the settled side. Measured:
- xG difference (sd ≈ 0.8 per match in the lab, so ≈ 40 matches resolve "just assembled", ≈ 400 "several signings"
  and ≈ 1,500 "new manager"); goal difference as a check on the long runs;
- coordination errors: double presses, no press, duplicate runs, a broken line;
- pressing success and receptions behind the line;
- passes within pairs.

What the evidence implies for the model:
- **Familiarity is small next to ratings.** The floors of f(practice) and f(partners) and the role-familiarity term
  must keep "just assembled" near a quarter of a goal. Before calibrating, convert the targets into rating points
  with the gap tables.
- **Role and partners weigh about equally** per unit of shortfall. This rests on the one study that measures both
  [HUCKMAN09], which is outside football; no football study isolates role familiarity. Practice weighs less.
- **It shows in passing between pairs, not in team completion.** Players who know each other pass to each other
  more, at about a 10 % scale (Grund 2016; Grohsjean et al., second-hand), and pairs with a passing history complete
  more [LECOZ25]. Team pass accuracy is unrelated [PASARAKONDA25]. Weak evidence puts the effect in goals scored
  rather than conceded [VANVUGT08].
- **Time course** (for the manager game):
  - Partners: 1 − exp(−matches together / 20), a proposed form fitted to two findings: chemistry is much weaker
    beyond ≈ 50 matches [BRANSEN20], and the benefit peaks at ≈ 67–80 prior appearances [FRANCK11].
  - Practice: a preseason is enough. After a mid-season change, any cost is gone within 15–20 matches [BRYSON21].

**Regression:** lab goals, the gap tables, `styles`, the headless match time.

## 16. Migration

Each phase sat behind a switch (`AiConfig::brain_model = Legacy | Individual`) and was compared on the benches before the
next one started. Phase G removed the legacy code and the switch. Defence comes first (second review). The team brain decides
most out of possession, and attack then builds on defenders who respond individually.

- **0. Attributes.** Done 2026-10-31 ([attributes.md](attributes.md) section 8).
  - The palette from [attributes.md](attributes.md): converted squads, effective values, duel and execution skills.
  - Check: the benches move only within noise.
- **A. Brains without a behaviour change.** Done 2026-10-31 (section 4, "In code").
  - The per-player brain struct and the world cache.
  - The team brain's computations evaluated per player on perfect beliefs.
  - Check: the benches match today within noise. Result: identical, not just within noise. All 40 lab matches have
    the same event-log hash as before, and the examples that read brain state print the same output.
  - The `brain_model` switch is not built yet. Phase A reproduces the team brain exactly, so there was nothing to
    switch between. It comes with the first phase that changes behaviour: `Team` then means the legacy logic, which
    is now evaluated per player.
- **B. Roles.** Done 2026-10-31.
  - The 14 roles of section 5.2 with their catalog entries; formation files with two roles per slot (5.3).
  - Duties replace `att_roles` / `def_roles` as inputs.
  - Check: today's 4-4-2 and 4-3-3, mapped onto the new roles, stay within noise on the benches. Result: identical.
    All 40 lab event-log hashes are unchanged.
  - In code: `team::role` (`Role`, `Unit`, `Lane`, `Duty`, `Movement`, `RoleDef`), `data/roles/*.ron` (ROLE-01 …
    ROLE-14 in the catalog), `Slot::role(in_possession)` with `out_role` in the formation files, and the formations
    4-4-2, 4-3-3, 4-2-3-1, 3-4-2-1, 3-4-3 and 3-5-2 (`team::FORMATIONS`). The brains' per-player assignments were
    renamed `att_duties` / `def_duties`. Their inputs are role duties now, not role names: the DEF-02 balance player
    and the BOX-04 back line come from `LastLine`, TR-05 is claimed first by `RestDefence`, and ATT-18 runs in
    behind come from the movement prior. Role familiarity (`familiarity: [f32; 2]` per player, from the squad's
    `familiarity` default and `role_familiarity` per role) replaced `attunement`; a player uses the familiarity of
    the role active in the current phase.
  - Tests: `every_formation_plays` (`tests/match_sanity.rs`), `rest_defence_is_claimed_by_its_roles`
    (`tests/brains.rs`), formation and role parsing unit tests.
- **C. Defence.** Done 2026-10-31.
  - The movement executor (paths, speed profiles, arrive-at-time), triggers and hysteresis.
  - Out-of-possession movements; duty claims for DEF-01…03, the line-leading duty, marking zones.
  - Counter-press and loose-ball decisions per player; denial values.
  - Check: the existing defending scenario tests and benches hold; test 10 for the defending duties. Result: `Legacy`
    reproduces phase B's 40 lab hashes; `Individual` (the default) moves the lab and the defending benches within
    noise (goals 2.75 → 2.55, tackles, duels, block, long balls and off-ball speed unchanged).
  - In code: `ai/movement.rs` (`Cue`, `Plan`; `Player::plan`): out of possession an outfield player re-chooses only at
    a trigger (possession, duty, press, phase, pass struck, new carrier) or the periodic check (`movement_check_s`,
    sooner for a reliable reader) and otherwise follows his movement at reaction-time re-reads. `Beliefs::line_leader`
    / `line_step` (BLK-02, `blk02_call_hold_s`). Denial-value claims, `DefDuty::Track` with handover
    (`def04_track_speed`, `def04_handover_distance`, per team `DefendingInstructions::track_speed`), mark velocity
    feed-forward (DEF-04, IND-04). Arrive-at-time for the press was already the chaser's brake logic (PRS-02).
    Counter-press and loose-ball choices were already per player and stay as they are until phase F gives players
    different readings.
  - Tests: test 10 (`tests/brains.rs`), DEF-04 runner tracking and handover, BLK-02 call hysteresis
    (`tests/principles.rs`).
- **D. Possession, front line.** Done 2026-10-31 (docs/design.md "Front-line movements, phase D").
  - **Pin, check, drop, run in behind**, with offer and space value; look-ahead levels 0–1.
  - Scenario tests 1, 2, 3, 8, 9 and 15 (`tests/principles.rs`); tests 4 and 5 need the midfield movements and move
    to phase E. Test 8 checks values: with realistic top speeds (88 vs 45 speed ≈ 9.5 vs 8.5 m/s) both profiles win
    an open race in behind, so the runner's advantage shows as a larger value of the run over the ball to feet.
  - In code: `ai/offer.rs` (`Passer`, `offer`, `expected_best`, `marker_of`, `after_response`),
    `positioning::front_movement` / `front_scores` / `FrontScore`, `movement::Decided` / `plan_after` / `lookahead`,
    the through ball into a run's offer in `on_ball::passes`, `analysis::in_vision_from`.
  - Benches: goals and xG at the real level; positions lag behind the targets after transitions; shots too many
    (selection); off-ball speed 3.73 m/s.
- **E. Possession, midfield and full-backs.** Done 2026-10-31 (docs/design.md "Possession movements, phase E").
  - **Show, in to out / out to in, overlap / underlap, invert, drop into the back line, step into midfield**; duty
    claims across all roles in possession; the back-three and back-five formations.
  - In code: `positioning::possession_movement` for every outfield player in possession (rest defence included,
    with its own repertoire), `Spots` / `offer_points` / `score_moves` / `MoveScore` / `move_scores` /
    `choose_move`; `offer::responded` (look-ahead level 1 for team-mates' movements: their markers follow them),
    `offer::keep_value` with the first defender's approach, ATT-10 screening, `offer_uncertainty`; the TR-05 claim
    by situation and instructions; `overlap` and `invert` player instructions (viewer sliders); the layer-3 choice
    noise kept until the next trigger (`Plan::noise`). **Decoy** has no movement of its own: the space value of a
    check, drop or run is what a decoy is worth.
  - Tests: 4, 5, 7 and 14 (`tests/principles.rs`), 13 and test 10 in possession (`tests/brains.rs`); ATT-10 and
    ATT-14 with perfect readers. Test 6 moved to M5 (BLK-03, see section 15).
- **F. Reading.** Done 2026-10-31 (docs/design.md "Reading team-mates, phase F").
  - Understanding, expectation, reading delays, knowing team-mates, instruction load.
  - Team data (practice, partners) and the familiarity presets.
  - Scenario tests 11–12 and the familiarity bench.
  - In code: `ai/reading.rs` (`load`, `familiarity`, `understanding`, `delay`, `receive_delay`, `estimated`, `seen`);
    `brain::assign_duties` / `read_team_mates` / `view`; `Beliefs::concept_def` / `concept_att` / `claim_since`;
    `Plan::concept`; `Player::instruction_familiarity`; `team::{Practice, Partners, Settledness}`; the pending
    receiver in `Simulation` and the set-up read (ATT-24).
  - Added to the design: the receiver's anticipation (section 7, "Reading the passer"). Claims and movements alone
    did not move the bench; pair passing is where the research places the effect.
  - Deferred to M6: estimates for the loose-ball chaser and the IND-07 challenge (they need the "mine" call to
    correct), reading the BLK-02 line call (exact cue until perception).
  - Benches: assembled +0.31 xG, signings +0.01, new system +0.07 (targets +0.25, +0.08, +0.04; ± 0.06 each).
- **G. Clean-up.** Done 2026-10-31 (docs/design.md "Clean-up, phase G").
  - Removed the team brain's legacy decisions, `brain_model` and the 13 parameters only they read; the catalog
    translations describe the movement model, with a history line per removed term.
  - Regression: the 40 lab matches are bit-identical to phase F; tests and clippy pass.
- **M4. Wide play.** Done 2026-10-31 (docs/design.md "Wide play, M4"). The box run (FT-03); crosses priced as
  balls over the lane with an aerial contest at the target (FT-02, `offer.rs`); CRS-01's line drop; the GK-04 claim
  margin; the touchline trap (PRS-05); the outnumbered first defender (IND-05).
- **M5. Instructions.** Done 2026-10-31 (docs/design.md "Instructions and the remaining tactics, M5"). BLK-03 and
  test 6; ATT-19, TR-02 and TR-03 come out of the value model; `TeamOption` and `PlayerPreset`; test 12 checks the
  reading delays.
- **M6. Full layer 3.** Done 2026-11-01 (docs/design.md "Full layer 3, M6"). Calls (claim, staying back, line call,
  "mine", "man on") reach team-mates at their next update; scanning (ATT-05) limits what a player sees of the
  opponents; concentration lapses; fatigue with pacing of positional movements; event-driven confidence.
- **M7. Laws and set pieces.** Done 2026-11-01 (docs/design.md "Laws and set pieces, M7"). Offside, fouls, cards and
  advantage, added time, substitutions, set-piece positions (`ai::set_piece`), the tactical foul, blocks and
  deflections. Set pieces are positions, not brains: once the ball is in play, the brains take over.

Beliefs stay perfect through phase E, so each phase can be checked against the optimum. Phase F then adds the
stat-based errors. Only when the perfect case is right can errors be traced to the reading model.

## 17. Open questions

Settled in the second review: match reading (intelligence), calls (M6, capacity model), familiarity (tested by
levels), roles (section 5), order (defence first). Settled afterwards: the attribute palette
([attributes.md](attributes.md), 28 ratings) and the 14 roles with two roles per slot (5.1, 5.2).

No open questions remain. The familiarity effect size, the last one, was researched on 2026-10-31
([familiarity-research.md](tactics/familiarity-research.md)), and section 15 now carries its targets. The effects are
small once team quality is controlled: "just assembled" ≈ +0.25 goals per match for the settled side, "new manager,
new system" ≈ +0.04.

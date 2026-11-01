# Tactical principle catalog

This catalog is the **source of truth for layer 1** (tactically optimal behaviour) of the match engine.
No layer-1 behaviour may exist in the engine without an entry here.

## Conventions

- **ID**: stable rule ID, referenced in engine code (doc comments, config field names) and tests.
- **Status**: `v1` = implemented in the first implementation pass (the coverage test requires the ID to appear in `crates/engine/src`); `deferred(Mx)` = planned for milestone Mx; `metric` = used only for phase labelling / calibration statistics.
- **Source says**: text in quotation marks is a verbatim quote, everything else is a paraphrase. Quotes marked "(transl.)" are our English translations of German sources. Source keys refer to [`sources.md`](sources.md).
- **Translation (ours)**: how the simulator implements the principle. This is *our* engineering interpretation, not the source's.
- **Parameters**: `sourced` = the number comes from the cited source; `calibrated` = the source is qualitative, the value is our starting guess and is tuned in the lab (`match_lab`) against the calibration targets at the end of this file. Parameter names match fields of `TacticsConfig` in `crates/engine/src/config.rs`. They are the layer-1 values. Parameters that set a style of play are also resolved per team, so a manager instruction can override them (defending: `DefendingTactics` in `crates/engine/src/team/tactics.rs`).
- Coordinates: team frame, attacking towards +x, own goal line at x = −52.5 m, pitch 105 × 68 m.

---

## Framework

### PH-01 · Phases of play (in possession)
Status: v1 · Phase: all
- **Source says:** FIFA defines in-possession phases: build-up (opposed / unopposed) — "a combination of short passes between team-mates, mostly from side to side, with the aim of progressing the ball forward through the thirds"; progression — "advance the ball into the final third … by vertical passes that break the opponents' lines, or by a player carrying the ball forward"; final third — "possession of the ball in the attacking third … where the aim is to finish the attack"; counter-attack — "regains possession and immediately attacks … with speed and intensity"; plus long ball and attacking transition [FIFA-EFI p.2–3]. FFA's four main moments: ball possession (BP), ball possession opponent (BPO), transition BP→BPO and BPO→BP [FFA-NC slide 10]. KNVB team functions use the same split [KNVB-TF].
- **Numbers:** thirds of the pitch only.
- **Translation (ours):** `TeamPhase` in each player's beliefs (`ai::brain`): `InPossession{BuildUp | Progression | FinalThird | CounterAttack}`, `OutOfPossession{…see BLK-01}`, `Contested`. In-possession phase by ball third and the counter-attack decision (TR-04). Transitions are timers started at regain / loss.
- **Parameters:** `ph01_attacking_transition_s` = 6.0 (sourced, "6 seconds" transition window [FA-HUNTS]).

### BLK-01 · Out-of-possession phase definitions
Status: v1 · Phase: out of possession
- **Source says:** nine out-of-possession phases: high / mid / low press, high / mid / low block, counter-press, recovery, defensive transition. Mid-block: "at least eight outfield defending players in the middle third of the pitch, or the middle and defensive third of the pitch (a low-block occurs when eight out-field players are in the defensive third only)". "Stationary" = not moving forward or backward faster than 10 km/h; "organised" = stationary ≥ 1.6 s of the last 2.0 s with ≥ 4 players behind the ball and horizontal lines; the phase ends when the criteria fail for 0.4 s within 2.0 s. Defensive line height = deepest defender to own goal line; team length = deepest to highest outfield player [FIFA-WC22-MB, "Defining a mid-block"; phase names also in FIFA-EFI, which gives no numeric thresholds].
- **Numbers:** as quoted.
- **Translation (ours):** each player's brain chooses the team's *intended* defensive mode (counter-press, recovery, or a block of a given height with pressing on triggers). Independently, `stats` labels every 0.2 s sample with the EFI definitions above for calibration.
- **Parameters:** `blk01_block_min_players` = 8 (sourced); `blk01_stationary_speed` = 2.78 m/s (sourced, 10 km/h); `blk01_organised_window_s` = 2.0, `blk01_organised_min_s` = 1.6 (sourced); `blk01_min_behind_ball` = 4 (sourced).

---

## Roles (formation slots)

A role is what a formation slot asks of its player (docs/player-brain.md section 5, research in [formations-research.md](formations-research.md)). A slot names an in-possession and an out-of-possession role. Each role is defined in `data/roles/*.ron`: its unit and lane, the duties it holds or competes for (`team::Duty`), movement priors (`team::Movement`), the team-mates it reads first and the instructions it accepts. The duties replace hard-coded role checks in the brains (phase B): the last line (`LastLine`) gives the DEF-02 balance player and the BOX-04 back line, `RestDefence` claims TR-05 first, centre-lane roles hold the ATT-13 centre, and a role runs in behind (ATT-18) when that is one of its main movements (prior ≥ 0.5). Formations: `data/tactics/*.ron` (4-4-2, 4-3-3, 4-2-3-1, 3-4-2-1, 3-4-3, 3-5-2).

### ROLE-01 · Goalkeeper (GK)
Status: v1 · Phase: all
- **Source says:** "goalkeepers must be able to play with their feet" [FFA-NC p.43]; "Goalkeeper to act as sweeper." [FFA-433 s29].
- **Numbers:** none.
- **Translation (ours):** `Role::GK` (`data/roles/gk.ron`): unit Keeper, lane Centre; duties []; relations [CB, CBC]; instructions [`build_up_involvement`, `sweeping_height`].
- **Parameters:** movement priors [(Hold, 1.0)] (calibrated; used by the movement choice).

### ROLE-02 · Centre-back in a pair (CB)
Status: v1 · Phase: all
- **Source says:** "know what to do when the opponent plays with one striker or with two, when to close down and mark an opponent and when to drop off and give cover"; "should also be able to move into the midfield with the ball at their feet and create a numerical advantage" [FFA-NC p.44]. "Als hinterster Feldspieler dirigiert der Innenverteidiger die Abwehrarbeit" (as the deepest outfield player the centre-back directs the defending, transl.) [BDFL-PETER09 p.1]. "Akanji stepped into midfield and formed a double pivot with Rodri" [UEFA-UCL24 p.15].
- **Numbers:** none.
- **Translation (ours):** `Role::CB` (`data/roles/cb.ron`): unit Back, lane Centre; duties [LastLine, LeadLine, RestDefence, Cover, MarkZone]; relations [CB, FB, GK]; instructions [`step_into_midfield`, `follow_dropping_nine`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.3), (Cover, 1.0), (Mark, 0.8), (Press, 0.4), (Recover, 1.0), (StepIntoMidfield, 0.2)] (calibrated; used by the movement choice).

### ROLE-03 · Central centre-back of a three or five (CBC)
Status: v1 · Phase: all
- **Source says:** "centreback Ezri Konsa was the spare man and chief organiser" [UEFA-UCL25 p.44]; "the middle of the three centre-backs, Thiago Silva, is unmarked and free to pick passes and move forward" [FIFA-GI-CHE]; from a back five, centre-backs "konnten … sich absetzende Spitzen verfolgen, ohne hinter sich Räume zu öffnen" (could follow dropping strikers without opening space behind them, transl.) [BDFL-WM14 p.3].
- **Numbers:** none.
- **Translation (ours):** `Role::CBC` (`data/roles/cbc.ron`): unit Back, lane Centre; duties [LastLine, LeadLine, RestDefence, Cover, MarkZone]; relations [CBW, GK]; instructions [`carry_into_midfield`, `mark_the_nine`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.3), (Cover, 1.0), (Mark, 0.6), (Recover, 1.0), (StepIntoMidfield, 0.3)] (calibrated; used by the movement choice).

### ROLE-04 · Outside centre-back of a three or five (CBW)
Status: v1 · Phase: all
- **Source says:** "Three centre-backs in back line with wide ones free to step up" [UEFA-TR24 p.83]; "Wide centre-backs … stepped out to apply pressure"; "wide centre-backs were never afraid of following a player" [UEFA-UCL25 p.27–28]; "Die beiden Halbverteidiger besetzen in Ballbesitz die Außenspuren" (the two outside centre-backs occupy the wide lanes in possession, transl.) [DFB-AK-IV p.1].
- **Numbers:** none.
- **Translation (ours):** `Role::CBW` (`data/roles/cbw.ron`): unit Back, lane Half; duties [LastLine, RestDefence, Cover, StepOut, MarkZone]; relations [CBC, WB]; instructions [`overlap`, `underlap`, `stay_home`, `step_out`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.3), (Press, 0.6), (Mark, 0.8), (Cover, 0.8), (Recover, 1.0), (Overlap, 0.3), (Underlap, 0.3)] (calibrated; used by the movement choice).

### ROLE-05 · Full-back in a back four (FB)
Status: v1 · Phase: all
- **Source says:** "They are often responsible for creating width for their team in the final third using wide channel runs, overlapping runs" [FIFA-IFB-CWC25]; "Defensively sometimes they have a direct opponent (winger) which requires strong defensive 1 v 1 skills" [FFA-NC p.43]; "the full-backs are playing very close to their centre-backs to protect the inside space" [FIFA-TSG]; "the right full-back tucked in alongside his centre-backs, so they maintained a 3+2" [FIFA-IFB-CWC25].
- **Numbers:** none.
- **Translation (ours):** `Role::FB` (`data/roles/fb.ron`): unit Back, lane Wide; duties [LastLine, RestDefence, Width, MarkZone]; relations [W, WM, CB]; instructions [`overlap`, `underlap`, `invert`, `stay_low`, `high_wide`, `drop_into_back_five`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.3), (Overlap, 0.5), (Underlap, 0.3), (Invert, 0.2), (Press, 0.5), (Mark, 0.8), (Track, 0.8), (Recover, 1.0)] (calibrated; used by the movement choice).

### ROLE-06 · Wing-back in a back five (WB)
Status: v1 · Phase: all
- **Source says:** "three central defenders and two wingbacks combining in a compact five-man defence"; "In attack the wingbacks pushed forward and stayed wide, always available for the switch of play" [UEFA-TR20 p.23]; "For the full-back who's moving to wing-back, this change allows you to be more aggressive and get out to the wide player early" [UEFA-UCL25 p.37]; "left wing-back Michel Aebischer excelled by coming inside into the No10 position" [UEFA-TR24 p.27].
- **Numbers:** none.
- **Translation (ours):** `Role::WB` (`data/roles/wb.ron`): unit Back, lane Wide; duties [LastLine, RestDefence, Width, StepOut, MarkZone]; relations [CBW, WM, AM]; instructions [`invert`, `stay_deep`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.3), (Overlap, 0.4), (RunInBehind, 0.4), (Invert, 0.2), (Press, 0.6), (Track, 0.8), (Recover, 1.0), (BoxRun, 0.4)] (calibrated; used by the movement choice).

### ROLE-07 · Single pivot (DM)
Status: v1 · Phase: all
- **Source says:** "Brazil changed to a 1-4-3-3 with Casemiro (5) acting as the pivot/single defensive midfielder. With Guimarães (8) and Paquetá (20) protecting the space to the right and left ahead of him, it allowed him to hold and marshal the central space. Out of possession, he could orchestrate the press" [FIFA-WC26]; "By dropping their deep-lying central midfielder, Adrien Rabiot, between the two centre-backs" [FIFA-WC26-B3].
- **Numbers:** none.
- **Translation (ours):** `Role::DM` (`data/roles/dm.ron`): unit Midfield, lane Centre; duties [RestDefence, Cover, MarkZone]; relations [CM8, CB]; instructions [`drop_into_back_line`, `mark_the_ten`, `cover_full_back`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.8), (Cover, 1.0), (Press, 0.5), (Mark, 0.6), (Recover, 1.0), (DropIntoBackLine, 0.3)] (calibrated; used by the movement choice).

### ROLE-08 · Central midfielder in a pair (CMP)
Status: v1 · Phase: all
- **Source says:** "Their task is to stay centrally and support the back four during BPO as well as feed and support the attack in BP"; "One of the two should always join in to support the attack while the other one stays behind the ball to keep the defensive balance" [FFA-NC p.44]; "if either centre-midfielder came out to press, the wide midfielder needed to come in narrow" [FIFA-WC26]; "Jetzt ist der ballferne Sechser schräg im Rücken des Partners und kann diesen somit sichern" (now the far-side 6 is diagonally behind his partner and can cover him, transl.) [BDFL-PETER09 p.5].
- **Numbers:** none.
- **Translation (ours):** `Role::CMP` (`data/roles/cmp.ron`): unit Midfield, lane Centre; duties [RestDefence, Cover, MarkZone]; relations [CMP, WM]; instructions [`hold`, `advance`, `drop_into_back_line`, `step_out`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.8), (Check, 0.2), (RunInBehind, 0.2), (Press, 0.6), (Cover, 0.8), (Recover, 1.0), (DropIntoBackLine, 0.2), (BoxRun, 0.3)] (calibrated; used by the movement choice).

### ROLE-09 · Number 8 beside a single pivot (CM8)
Status: v1 · Phase: all
- **Source says:** Slovakia "pressed from the inside midfield player", "No8s, rather than the wingers, were the players jumping out" [UEFA-TR24 p.56]; Guimarães and Paquetá "protecting the space to the right and left ahead of him" [FIFA-WC26]; "midfield three working hard to cover the width" [UEFA-UCL25 p.27].
- **Numbers:** none.
- **Translation (ours):** `Role::CM8` (`data/roles/cm8.ron`): unit Midfield, lane Half; duties [StepOut, Cover, MarkZone]; relations [DM, FB, WB, W]; instructions [`press_trigger`, `run_beyond`, `stay`, `go_wide`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.8), (Drop, 0.5), (RunInBehind, 0.3), (InToOut, 0.3), (Press, 0.8), (Recover, 1.0), (BoxRun, 0.5)] (calibrated; used by the movement choice).

### ROLE-10 · Attacking midfielder between the lines (AM)
Status: v1 · Phase: all
- **Source says:** "Attacking midfielder (10) Jude Bellingham, occupies the half space between opposition midfield and defensive line"; "they often defended in a 1-4-4-2 mid-block, with Kane and Bellingham acting as the front two" [FIFA-WC26]; "two No10s in an attacking 3-4-2-1" [UEFA-TR24 p.35]; "a midfield position (not a 2nd striker)" [FFA-NC p.45].
- **Numbers:** none.
- **Translation (ours):** `Role::AM` (`data/roles/am.ron`): unit Midfield, lane Centre; duties [MarkZone]; relations [CF, ST2]; instructions [`lane`, `free_role`, `drop_to_build`, `run_in_behind`].
- **Parameters:** movement priors [(Hold, 1.0), (Drop, 1.0), (Show, 0.6), (RunInBehind, 0.6), (Decoy, 0.3), (Press, 0.6), (Recover, 0.6), (BoxRun, 0.7)] (calibrated; used by the movement choice).

### ROLE-11 · Wide midfielder in a midfield line (WM)
Status: v1 · Phase: all
- **Source says:** "the wide midfielder needed to come in narrow" [FIFA-WC26]; "creating a 1-4-4-2, with Vinícius Júnior on the left and Rodrygo on the right" [UEFA-UCL25 p.28]; "das Mitarbeiten nach hinten durch die zurückeilenden Außenmittelfeldspieler" (the wide midfielders sprinting back to help defend, transl.) [BDFL-PETER09 p.5].
- **Numbers:** none.
- **Translation (ours):** `Role::WM` (`data/roles/wm.ron`): unit Midfield, lane Wide; duties [Width, Cover, MarkZone]; relations [CMP, CM8, FB]; instructions [`tuck_in`, `stay_wide`, `track_runner`, `pass_on_runner`].
- **Parameters:** movement priors [(Hold, 1.0), (Show, 0.4), (InToOut, 0.4), (OutToIn, 0.5), (RunInBehind, 0.5), (Overlap, 0.2), (Track, 0.9), (Press, 0.6), (Recover, 1.0), (BoxRun, 0.5)] (calibrated; used by the movement choice).

### ROLE-12 · Winger or wide forward (W)
Status: v1 · Phase: all
- **Source says:** "Their starting position should always be high and wide."; "together with the full-back, protect and defend their designated wing" [FFA-NC p.45]; "Wingers … stay high and wide, stretching the play and/or pinning the opposition full-backs" [FIFA-WC26]; "you go outside when he goes inside" [UEFA-UCL25 p.23].
- **Numbers:** none.
- **Translation (ours):** `Role::W` (`data/roles/w.ron`): unit Front, lane Wide; duties [Width, Depth]; relations [FB, WB]; instructions [`wide`, `inside`, `press_outside_in`, `stay_high`, `track_back`].
- **Parameters:** movement priors [(Hold, 1.0), (Pin, 0.8), (RunInBehind, 1.0), (OutToIn, 0.6), (InToOut, 0.4), (Check, 0.4), (Press, 0.7), (Track, 0.4), (BoxRun, 0.8)] (calibrated; used by the movement choice).

### ROLE-13 · Lone centre-forward (CF)
Status: v1 · Phase: all
- **Source says:** a striker "that plays as high as possible"; pressing: "determining when and where to start pressuring the opponent" [FFA-NC p.46]; "Centre-forward, (9) Harry Kane drops down deep into centre midfield to create overload" [FIFA-WC26]; Number 9s "offer in behind" [FIFA-N9-WC22].
- **Numbers:** none.
- **Translation (ours):** `Role::CF` (`data/roles/cf.ron`): unit Front, lane Centre; duties [Depth, StartPress]; relations [AM, W]; instructions [`come_short`, `target_play`, `press_trigger`].
- **Parameters:** movement priors [(Hold, 1.0), (Pin, 1.0), (Check, 0.8), (Drop, 0.6), (RunInBehind, 1.0), (Decoy, 0.5), (Press, 0.8), (BoxRun, 1.0)] (calibrated; used by the movement choice).

### ROLE-14 · Striker in a pair (ST2)
Status: v1 · Phase: all
- **Source says:** "The ball-side forward's role was to get early, direct pressure on the centre-back receiving the ball, while the second forward actively positioned to get immediate pressure on the goalkeeper" [FIFA-WC26]; "Der Stürmer erhält das Zuspiel, legt auf den Sechser zurück und startet vor das Tor" (the striker receives, lays off to the 6 and runs towards goal, transl.) [DFB-TE-2ST p.3]; one striker offers, the other holds the depth [DFB-RP16].
- **Numbers:** none.
- **Translation (ours):** `Role::ST2` (`data/roles/st2.ron`): unit Front, lane Centre; duties [Depth, StartPress]; relations [ST2]; instructions [`offer_or_hold_depth`, `press_the_cb`, `screen_the_pivot`].
- **Parameters:** movement priors [(Hold, 1.0), (Pin, 0.9), (Check, 0.8), (Drop, 0.4), (RunInBehind, 1.0), (Decoy, 0.5), (Press, 0.8), (BoxRun, 1.0)] (calibrated; used by the movement choice).

---

## Out of possession — team defending

### DEF-01 · Defending principles set
Status: v1 · Phase: out of possession
- **Source says:** The FA lists the defending principles as "Delaying", "Compactness", "Cover and balance", "Pressing" and "Control and restraint" [FA-EFL]. UEFA B course: "deny space by delaying, creating depth, being compact … cover and balance … control and restraint - patience" [FA-UEFAB].
- **Numbers:** none.
- **Translation (ours):** every defender gets one role per team tick: `First` (pressure / delay), `Second` (cover), `Third` (balance), `Shape` (compact block, see DEF-05).
- **Parameters:** none.

### DEF-02 · Pressure, cover and balance
Status: v1 · Phase: out of possession
- **Source says:** "The closest defender to the ball applies pressure and angles their body shape to show the attacker to the outside." "The second defender provides support behind and inside to help prevent split passes", at "a distance that allows support if the first defender is beaten". "When the ball is wide, the furthest defender should provide balance by staying connected and covering space on the weak side" [FIFA-ZD]. "Covering is all about angles and distances … don't be too close" [FA-EFL].
- **Numbers:** not specified.
- **Translation (ours):** cover target = presser position + `def02_cover_distance` along the direction from ball to own goal centre, rotated `def02_cover_angle_deg` towards the pitch centre line (inside). Once the presser is no longer goal-side of the ball (beaten), the cover is measured from the contain point he was beaten from, so the cover stays where he can still support. The cover moves with the carrier and re-aims at his reaction time, like the first defender (IND-01), so he keeps the supporting distance when the carrier accelerates. Balance player = the third defender on the weak side keeps its block anchor (connected) instead of moving towards the ball.
- **Parameters:** `def02_cover_distance` = 6.0 m (calibrated); `def02_cover_angle_deg` = 30° (calibrated).

### DEF-03 · Role swapping and hand-over
Status: v1 · Phase: out of possession
- **Source says:** the second presser "provides cover and then becomes the first pressing player if the ball is played past the first" [FIFA-DU]. Horizontal and vertical hand-over, "Covering each other (diagonal)" [FFA-433].
- **Numbers:** none.
- **Translation (ours):** roles are reassigned every team tick by time-to-ball (`ai::reach`), with a hysteresis so roles do not flicker. Because pressure is applied goal-side (DEF-02, IND-01), the time is measured to the contain point `ind01_contain_distance` goal-side of the ball. That point moves with a running carrier, so the time is the time to *meet* it there (`time_to_meet`): a defender in the carrier's path takes over from a teammate chasing from behind, who cannot catch it. The same holds for choosing the second defender. In the counter-press (TRN-02) the time is measured to the ball itself. A first defender who has engaged the carrier keeps the role until the ball is played past him: while he stays between ball and goal (ahead of the carrier on his line to the goal centre, at most `def03_engaged_half_width` to the side, within `ind01_track_distance`, not recovering from a lost tackle), no teammate takes over on time alone. Reading (layer 3, docs/player-brain.md section 7): each defender resolves the roles from his estimates of his team-mates' times (a team-mate he does not know as one of average pace) and holds what he has read of their claims, so a side that does not know each other has moments with two or nobody pressing until one reads the other; of two who claim the same first-defender role, mark or track, the one who claimed it first keeps it.
- **Parameters:** `def03_role_hysteresis_s` = 0.3 s (calibrated); `def03_engaged_half_width` = 2 m (calibrated). Reading (calibrated against the familiarity research): `reading_delay_max_s`, `reading_practice_floor`, `reading_partner_floor`, `reading_load_floor`.

### DEF-04 · Zonal reference with man-orientation
Status: v1 · Phase: out of possession
- **Source says:** positioning is "guided primarily by the position of the ball, the positioning of their teammates" and space, with "more player-oriented actions in certain moments" [FIFA-ZD]. Players "get close to their mark as soon as they see the opponent's intention to play into their zone" [FIFA-PR, 10v10]. Most teams mix zonal and player-to-player marking [FA-EFL]. A centre-back who follows a striker coming short leaves "ein ungedeckter Raum vor dem eigenen Tor" (an unguarded space in front of the own goal); the correction is "Der Innenverteidiger lässt die Spitze zunächst ziehen und sichert damit weiter den Raum vor dem Tor" (the centre-back lets the striker go at first and so keeps securing the space in front of goal), and a second striker crossing behind "könnte … mühelos an ihn übergeben werden" (could easily be handed over to him) [BDFL-PETER09, "Positionstaktik im 6-gegen-6", Korrektur 2, pictures 1–3]. When the ball goes in behind, defenders "sprint back to mark, cover and defend their goal" [FA-EFL, "How to defend like England: space in behind"].
- **Numbers:** none.
- **Translation (ours):** the anchor is the zonal reference (formation slot shifted with the ball). An opponent inside a defender's zone radius becomes his marking reference (IND-04); when a pass towards that opponent starts, the marker closes immediately. A marker keeps his man until the man is `def04_release_margin` outside the zone (handover), so marking does not flicker at the zone's edge. Claims (docs/player-brain.md section 6): the free opponents are claimed most dangerous first, by denial value (the possession value where the opponent stands × how open the lane from the ball to him is), each by the nearest free defender whose zone holds him, so the most dangerous man is never the one left free because a defender happened to be nearer a harmless one. An opponent leaving his marker's zone *towards the own goal* at `def04_track_speed` or more is handed over to a team-mate standing goal-side of him within `def04_handover_distance`; with nobody there his marker tracks him (IND-04 marking point, moving with him) until a team-mate takes him over or the runner slows below half that speed. A runner coming short is not followed: he is let go and the zone is kept. The attackers read the same rule when they price the space a movement leaves (`offer::after_response`): a zonal marker follows a man into his offer, but lets one who comes short or across out of his zone go.
- **Parameters:** `def04_zone_radius` = 12.0 m (calibrated); `def04_release_margin` = 4.0 m (calibrated); `def04_track_speed` = 4.0 m/s (calibrated: a run, not a jog; per team `DefendingInstructions::track_speed`, `TeamOption::TrackEveryRunner` / `PassRunnersOn`, 0 = track every runner, 12 = never); `def04_handover_distance` = 3.0 m (calibrated).

### DEF-05 · Compactness
Status: v1 · Phase: out of possession
- **Source says:** WC2022 mid-block: "aligned with the tournament average (27.2m v 40.1m)" (length v width); team examples Saudi Arabia "39.6m wide x 22.5m long" (most compact), Morocco "40.75m wide x 23.5m long", Argentina width 40.5 m / length 29.1 m, France narrow at 38.4 m; compactness increased the longer the block lasted [FIFA-WC22-MB]. Al Ahly (one unusually compact team, opponents at 32.3 m): length 26 m in mid-block, 24.3 m in low block [FIFA-CWC]. "team total: 40 x 40 m", "distance between players: 10-15 m", "no mixing of the lines", "Keep the central spine always occupied" [FFA-433, provenance unverified].
- **Numbers:** as quoted.
- **Translation (ours):** outfield block size used by `ai::shape`: length by block height, width fixed. The widest out-of-possession formation slots sit on the block edge (normalised y ±1.0), so the block really spans `def05_width` (before, the slots stopped at ±0.85 and the block measured ≈ 34.5 m wide). Positioning penalises candidates outside the block. The block moves as a unit: its lines and centre follow their new positions at no more than `def05_shift_speed` (they do not jump with every pass, which made the whole team re-run at once). Near the own goal BOX-04 narrows and shortens the block and lets its lines drop faster.
- **Parameters:** `def05_length_high` = 30.0 m (calibrated); `def05_length_mid` = 27.2 m (sourced, WC2022 tournament average); `def05_length_low` = 24.3 m (calibrated, starting from the single Al Ahly example); `def05_width` = 40.1 m (sourced, WC2022 tournament average); `def05_shift_speed` = 3.0 m/s (calibrated).

### DEF-06 · Ball-side shift and weak side
Status: v1 · Phase: out of possession
- **Source says:** "The players furthest from the ball shuffle over to maintain the distances, leaving the opposite flank open"; the block moves at the speed of the ball [FIFA-MB]. "The defending player on the opposite side … must tuck in" [FIFA-PR]. "Defending players on the ball-far side should prioritise closing spaces on the inside and not worry about opposition players on the weak side" [FIFA-PR, 6v6+2]. Do not tuck in too far because of the switch [BDFL-PETER].
- **Numbers:** none.
- **Translation (ours):** block centre y = `def06_shift_factor` · ball y, clamped so the block (width DEF-05) stays on the pitch; the weak-side flank is left open. The tuck is limited because the block width is fixed (the far full-back stays ≈ 40 m from the ball-side full-back, enough to reach a switch). Near the own goal BOX-04 takes over: a narrower block that shifts less.
- **Parameters:** `def06_shift_factor` = 0.65 (calibrated).

### DEF-07 · Protect the centre, direct play
Status: v1 · Phase: out of possession
- **Source says:** "Defending players must constantly think about protecting the central channel as a priority"; steer opponents to "lower-risk parts of the pitch (generally the flanks)" [FIFA-MB]. Block the inside route individually [FIFA-ZD]. Team option to force play wide or central [FFA-433]; steering wide "makes goal protection easier", from a diamond steer inside [BDFL-PETER].
- **Numbers:** none.
- **Translation (ours):** team parameter `channel` ∈ {`Wide`, `Inside`} (default `Wide`, the sources' general case) sets which side the first defender blocks (IND-02).
- **Parameters:** default channel `Wide` (sourced as general case).

### BLK-02 · Line height versus pressure on the ball
Status: v1 · Phase: out of possession
- **Source says:** "The less pressure on the ball, the deeper the covering angle"; "As pressure is applied to the ball, then the defence can move up" [FA-UEFAB]. "If the player in possession is closed down, the defence may push up or hold … If … not closed down, there is a danger of a long ball and the defence must therefore be ready to retreat" [FIFA-MB]. The line must "hold, step or drop" [FIFA-ZD]. On a clearance or back pass "the whole team moves up" [FFA-433].
- **Numbers:** none.
- **Translation (ours):** the ball carrier is "pressured" if a defender is within `blk02_pressure_radius`, goal-side. The defensive line must stay at least `blk02_gap_pressured` (pressured) or `blk02_gap_unpressured` (not pressured) behind the ball, otherwise it holds the block-height line (BLK-05). The drop exists to protect the space for a long ball in behind; near goal that space belongs to the goalkeeper, so the gap is capped at `blk02_max_drop_share` × the ball's distance from the goal line (the line does not collapse onto the six-yard box while the ball is outside the area). After a back pass or clearance the line steps up to the block height. The call has an owner: "Als hinterster Feldspieler dirigiert der Innenverteidiger die Abwehrarbeit und gibt z. B. Pressingkommandos" (as the deepest outfield player the centre-back directs the defensive work and gives, for example, pressing commands) [BDFL-PETER09, "Mannschaftstaktik"]. The line leader is the player with the line-leading duty (ROLE-03/04 centre-backs; the last-line duty if no slot has it) nearest the block's centre, kept unless another is 3 m nearer. He changes the call (step or drop) only once the reading (pressure on the ball, or a back pass) has held for `blk02_call_hold_s`, at once on a back pass, so a flicker of pressure moves nobody.
- **Parameters:** `blk02_call_hold_s` = 0.4 s (calibrated: about two reaction times); `blk02_pressure_radius` = 2.5 m (calibrated); `blk02_gap_pressured` = 8.0 m (calibrated); `blk02_gap_unpressured` = 16.0 m (calibrated); `blk02_max_drop_share` = 0.5 (calibrated: the deepest outfield defender at open-play passes from the central corridor stands at a drop share of 0.39–0.43 of the ball's distance to the goal line with the passer free and 0.33–0.38 with an opponent within 3 m, ball 18–30 m out, StatsBomb 360; the engine gives 0.39–0.45 / 0.35–0.37, against 0.30–0.33 / 0.29–0.32 at the former 0.4; `final_third` bench).

### BLK-03 · Stepping out of the line
Status: v1 · Phase: out of possession
- **Source says:** a defender steps out to mark a player between the lines while teammates "shuffle across", and returns when the threat is gone or handed over [FIFA-MB].
- **Numbers:** none.
- **Translation (ours):** an opponent more than `blk03_front_m` in front of the own line has dropped between the lines. A back-line defender follows him (keeps or claims the DEF-04 mark) only up to his `step_out` in front of the line, only while no other line defender is out, and only if a line team-mate whose slot is within `blk03_shift_m` of his can shift across. Otherwise he passes him on to the midfield, who take him if one is free. Whether to go is decided as the man drops: one already out keeps following within his reach. While he is out, the back-line players on the side of his slot away from the ball shift into it by `shuffle` of the gap to the nearest of them ("shuffle across"). Instructions `step_out` and `shuffle` (`TeamOption::StepOutToMidfielders` / `HoldTheLine`).
- **Parameters:** `blk03_front_m` = 4 m, `blk03_step_out_m` = 12 m (the DEF-04 zone radius), `blk03_shift_m` = 16 m, `blk03_shuffle` = 0.5 (all calibrated).
- **History:** before M5 a centre-back followed any opponent in his 12 m DEF-04 zone, with no permission and no shuffle.

### BLK-04 · Diagonal retreat on a switch
Status: v1 · Phase: out of possession
- **Source says:** during a long diagonal ball in flight, "the complete team retreats diagonally" (transl. of "Die komplette Mannschaft weicht in der Flugphase des Balles schräg zurück") — behind the ball and towards its new location at the same time [BDFL-PETER].
- **Numbers:** none.
- **Translation (ours):** while a long ball is in flight, until it lands, the player brains use the predicted landing point instead of the current ball position as the block reference. The block's shift-rate cap (`def05_shift_speed`, DEF-05) is raised for the flight so that the block reaches its new place when the ball arrives ("the block moves at the speed of the ball" [FIFA-MB]).
- **Parameters:** `blk04_min_flight_s` = 0.8 s (calibrated).

### BLK-05 · Block and press heights
Status: v1 · Phase: out of possession
- **Source says:** The FA distinguishes high press, mid-press (from the attacking middle third) and low press (from halfway) [FA-DNA]. Midfield pressing ≈ "15 metres on either side of the halfway line" (transl. of "ca. 15 Meter beiderseits der Mittellinie") [BDFL-PETER]. In a high press the back line is "often just inside the opponents' half" [FIFA-HP]. (The 6v6 high-block session of [FIFA-PR] uses a half-pitch area with its own offside line, so it gives no full-pitch line height.)
- **Numbers:** as quoted.
- **Translation (ours):** block height `High | Mid | Low` (layer-1 default `Mid`, manager can override) sets the reference line x and the pressing zone (the first defender engages only when the ball is inside the zone, otherwise delays, PRS-07).
- **Parameters:** `blk05_line_high` = +2.0 m (sourced, "just inside the opponents' half"); `blk05_line_mid` = −20.0 m (calibrated); `blk05_line_low` = −35.0 m (calibrated); `blk05_press_zone_high` = +52.5 (whole pitch); `blk05_press_zone_mid` = +15.0 m (sourced, Peter ±15 m around halfway); `blk05_press_zone_low` = −17.5 m (calibrated, own third).
- **Bench (M8, `data/experiments/high_line_fast_strikers.ron`, 500 matches per arm):** Northbridge's high block against its layer-1 block gains +0.47 ± 0.16 points, +10.1 shots and +0.84 xG per match, and the opponents' xG falls by 0.20 ± 0.06 (offsides against them +2.1). Harbour's two strikers at speed 94.7 / quickness 89.5 add only +0.14 ± 0.07 xG, and the interaction (high line × fast strikers) is nil: +0.03 ± 0.09 xG against. The space behind a high line costs nothing yet, which fits the short counter-attacks (≈ 33 m against ≈ 70 m) and the few goals from moves of 1–5 passes (≈ 27 % against 53 %): open, see docs/design.md, "Calibration lab, M8".

### PRS-01 · Pressing triggers
Status: v1 · Phase: out of possession
- **Source says:** heavy touch, hesitation, "facing their own goal", slow pass [FA-EFL pressing zones]; opponent looking down, ball not under control, "press immediately while the attacker waits to receive" [FA-EFL speed of press]; "bad touch, a back pass or a receiver facing their own goal" [FIFA-NEC]; poor first touch, weak passes [FIFA-1V2]; collective moments: goalkeeper in possession, long ball to a wide player, clearance, back pass, after which "the whole team moves up" [FFA-433]; closed body stance → double up [BDFL-PETER].
- **Numbers:** none.
- **Translation (ours):** a trigger set evaluated at pass release and at the receiver's first touch: back pass, poor control (control quality below threshold), receiver facing own goal, slow pass, high ball (an opponent's lofted pass in flight: its receiver has to bring it under control while he waits for it), ball close to the touchline, goalkeeper in possession. With a trigger the first defender (and second, PRS-05 later) press even outside the press zone, but at most `prs01_zone_extension` beyond it: a lone presser far ahead of a mid-block is not what the sources describe ("the whole team moves up" [FFA-433]; collective block step-up deferred to PRS-05).

  **Double-up** (2026-11-01): "Der Passempfänger steht geschlossen zum Spiel und kann von dem herausrückenden Sechser und dem nach hinten doppelnden Stürmer attackiert werden. Der Sechser versucht, den Gegner nicht aufdrehen zu lassen und schließt den Weg zum Tor, der nach hinten doppelnde Stürmer zielt auf den Ballgewinn ab" (the receiver stands closed to play and can be attacked by the six stepping out and the striker doubling back; the six keeps him from turning and closes the way to goal, the doubling striker goes for the ball); on a pass to the opposing winger "der Sechser … gemeinsam mit dem Außenverteidiger doppeln kann" (the six can double together with the full-back) [BDFL-PETER, pressing variant 2]. Translation: when the carrier (not a keeper) faces his own goal and a first defender is engaged on him, the team-mate who can reach the point `prs01_double_distance` in front of him (on the side he faces) soonest, within `prs01_double_time_s`, takes the duty `DefDuty::Double`: he closes in there and goes for the ball. His tackle threshold is BOX-03's covered one (`box03_tackle_min_p`) while the first defender is goal-side within `box03_cover_radius`, because the first defender keeps the way to goal shut. Each player claims the duty from his own beliefs, like the first defender (section 7 of docs/player-brain.md).
  - Bench (6 films, 2026-11-01, StatsBomb 40 matches with real counts × 1.62): forwards' on-ball spells of 6 s or more 11.6 per team-match before (real 3.7), 12.7 with the double-up alone (it applied in 2.1 % of on-ball ticks: holders faced the opponents' goal), 9.9 with the ATT-25 shielding stance (doubled 2.8 %); pressed forwards' share of spells of 6 s or more 5.8 % (real 4.3 %). The rest of the gap per match is volume: engine forwards receive under pressure about twice as often (long balls, docs/design.md "Watch round").
- **Parameters:** `prs01_zone_extension` = 15.0 m (calibrated); `prs01_slow_pass_speed` = 8.0 m/s (calibrated); `prs01_touchline_zone` = 27.0 m (|y| beyond; calibrated); `prs01_poor_touch_quality` = 0.5 (calibrated); `prs01_trigger_duration_s` = 2.0 s (calibrated: how long a detected trigger keeps the press on); `prs01_double_time_s` = 2.0 s (calibrated: how far the doubler may be, in time to the point in front of the carrier); `prs01_double_distance` = 1.0 m (calibrated: the doubler's point in front of the carrier).
- **Instruction:** per team `DefendingInstructions::double_time` (0 = no doubling); "double up on a player with his back to goal" / "don't double up" (`TeamOption::DoubleUpOnBackToGoal` ×1.5 / `NoDoublingUp`).

### PRS-02 · Press timing
Status: v1 · Phase: out of possession
- **Source says:** "Arrive on the receiving player's first touch and get touch-tight"; "move towards the receiving player as soon as the ball leaves the passer's foot"; cover players "join the press as soon as the ball starts to travel" [FIFA-DU].
- **Numbers:** none.
- **Translation (ours):** at pass release the defender with the smallest time-to-reach the reception point becomes first defender immediately (no waiting for the team tick) if he can arrive no later than `prs02_arrival_slack_s` after the ball. An outfield defender can go for a dropping ball up to his header height. One who is first to the ball plays it; one who is later than the receiver (within the slack) goes to the receiver's reception point, timed for the first touch, and eases off when he would be early, instead of chasing to where the ball would run on past the receiver. Whoever is first defender while an opponent's pass is travelling, and the cover joining him (DEF-02), take its reception point as their reference: the contain point, the goal-side check and the cover point are set against where the receiver will be, not against the ball on its way there. Before, a first defender who was not the PRS-02 chaser contained the ball in flight and ended a median 14.4 m from the receiver of a 25–45 m forward pass; now 12.0 m, about as close as anyone could sprint (`long_defend` example).
- **Parameters:** `prs02_arrival_slack_s` = 0.4 s (calibrated).

### PRS-03 · Approach run and deceleration
Status: v1 · Phase: out of possession
- **Source says:** "Curving their run can help them cut off passing lanes"; the angle "can force an opponent inside or outside" [FA-EFL]; "avoid straight-line chasing" [FIFA-NEC]. "Slow down enough so their opponent can't use the defender's momentum"; side-on, knees bent [FA-EFL]. Not too wide, not too fast, not too "greedy" [BDFL-PETER]. FIFA's pressure score is higher when the defender is in front of the ball carrier [FIFA-EFI p.12].
- **Numbers:** none.
- **Translation (ours):** the presser runs to a waypoint offset towards the side to be blocked (curved approach), then to the contain point; locomotion brakes within `prs03_decel_distance`.
- **Parameters:** `prs03_decel_distance` = 4.0 m (calibrated); `prs03_curve_offset` = 2.0 m (calibrated).

### PRS-04 · Cover shadow and lane blocking
Status: v1 · Phase: out of possession
- **Source says:** the approaching defender limits pass options "through his cover shadow" [BDFL-KOHF]. Two strikers staggered: the ball-near one presses, the partner drops slightly [BDFL-PETER]; "one presses and the other screens the central lanes" [FIFA-NEC]. "Mark/pressure the players closest to the ball and leave the ones which are the furthest away free" [FFA-NC].
- **Numbers:** none.
- **Translation (ours):** positioning score term rewarding candidate points close to the passing lane from the ball to dangerous nearby opponents (lane blocking / screening), weighted by opponent proximity to the ball.
- **Parameters:** `prs04_lane_weight` = 1.0 (calibrated); `prs04_lane_radius` = 3.0 m (calibrated).

### PRS-05 · Pressing traps and numbers
Status: v1 · Phase: out of possession
- **Source says:** "A pressing trap invites the opposition to play in a specific way" [FIFA-HP]; goal-kick press: cut the pass back to the goalkeeper, show the centre-back to the touchline [FIFA-PR]; UEFA "team press" = "five players or more, or which takes place in the high block"; Ajax used 2–3 pressers in 58 % of presses; Inter began 82 % of presses within two opposition passes [UEFA-PI]; numerical advantage around the ball [FIFA-PR].
- **Numbers:** 2–3 pressers in 58 % of Ajax's presses, five or more = "team press" [UEFA-PI] (the PRS-06 calibration target).
- **Translation (ours):** in a press (BLK-05 / PRS-01) with the carrier in the touchline zone (`prs01_touchline_zone`), the covering defender (DEF-02) closes the side the first defender leaves open: down the line when the team shows the carrier wide (DEF-07 `Channel::Wide`), inside otherwise, `prs05_trap_distance` from the carrier and moving with him. The first defender, the cover and the touchline leave the carrier no way out, so the first defender does not delay as outnumbered there (IND-05). `prs05_trap` is how far the cover goes from his DEF-02 point towards the trap point; instruction `DefendingInstructions::trap` (`TeamOption::TrapAtTheTouchline` / `NoTouchlineTrap`). The goal-kick press (cut the pass back to the keeper) has no routine of its own: at a goal kick the opponents keep outside the penalty area (SP-05) and press by PRS-01 once the keeper has the ball.
- **Parameters:** `prs05_trap_distance` = 4.0 m (calibrated); `prs05_trap` = 1.0 (calibrated; 0 = no trap).
- **Bench:** pressers 1 / 2–3 / 4+ = 52 / 43 / 6 % (52 / 42 / 5 % before; the trap applies only at the touchline) against 58 % with 2–3 and 25 % with 4+: the press still commits too few players around the ball (open).

### PRS-06 · Who counts as pressing (tracking definition)
Status: metric · Phase: out of possession
- **Source says:** in the final third, a player is pressing if within 1.5 m of the ball; or faster than his team average and > 15 km/h within 20 m (moving toward ball and goal) or 6 m (toward the ball only); or > 20 km/h within 25 m; or > 15 km/h moving forward no deeper than 10 m from the ball; excluded if four times closer to the ball than the nearest attacker [FIFA-CWC 2025].
- **Translation (ours):** used by `stats` to count pressers per press for calibration against the 2–3 presser target.

### PRS-07 · No trigger, or press beaten
Status: v1 · Phase: out of possession
- **Source says:** "If there isn't a trigger to press, they should slow down and delay" [FA-EFL]. "If can't pressure the ball: stop moving forward" [FFA-433]. "After an unsuccessful attempt … pressing players must immediately drop back into shape" [FIFA-PR].
- **Numbers:** none.
- **Translation (ours):** outside the press zone and without trigger the first defender only delays: he holds a contain point `prs07_delay_distance` goal-side of the ball, but does not step more than `prs07_front_step` beyond the front of the block ("stop moving forward"), so a ball deep in the opponents' half is not chased. If the ball carrier gets past him (the presser is no longer goal-side), he drops into recovery (IND-06).
- **Parameters:** `prs07_delay_distance` = 4.0 m (calibrated); `prs07_front_step` = 3.0 m (calibrated).

## Out of possession — individual defending

### IND-01 · Closing down
Status: v1 · Phase: out of possession
- **Source says:** "Before the receiving player takes a touch, close them down quickly"; the 1v1 principles are direct, delay, deny [FIFA-ZD]. Close the distance quickly; if the dribbler comes at speed, match it backwards; low centre of gravity [BDFL-KOHF].
- **Numbers:** none.
- **Translation (ours):** the first defender sprints until PRS-03 deceleration, then holds the contain point while matching the carrier's velocity (backwards when he runs at goal): the contain point moves on with the carrier, and the defender arrives at it relative to that motion (pursuit, `Player::target_vel`). Within `ind01_track_distance` of the carrier he re-aims at his reaction time instead of his decision interval, so he mirrors the carrier's changes of direction.
- **Parameters:** `ind01_contain_distance` = 1.8 m (calibrated); `ind01_track_distance` = 5 m (calibrated).

### IND-02 · Body shape and showing
Status: v1 · Phase: out of possession
- **Source says:** side-on, channelling the attacker onto his weaker side; approach "toward the receiver's inside thigh" to force him outside [FIFA-ZD]. Full-backs show outside; otherwise inside/outside is a coaching choice [BDFL-KOHF].
- **Numbers:** none.
- **Translation (ours):** the contain point lies on the ball–goal line, shifted `ind02_show_offset` towards the side that is blocked (inside for channel `Wide`, DEF-07), so the carrier's free path is the other side.
- **Parameters:** `ind02_show_offset` = 0.6 m (calibrated).

### IND-03 · Tackle or contain
Status: v1 · Phase: out of possession
- **Source says:** "If it's possible to tackle … time it correctly rather than diving in", otherwise "slow down and delay" [FA-EFL]. "Direct play without overcommitting beyond the ball" [FIFA-ZD]. Can he win the ball? If not, force it backwards [FA-UEFAB].
- **Numbers:** none.
- **Translation (ours):** the first defender attempts a tackle only when the ball is within tackle reach and the duel model's win probability is at least `ind03_tackle_min_p` (or the carrier is about to shoot); otherwise he contains. Containing presumes he stays between the carrier and his goal ("slow down and delay"): a carrier who is about to get past him (goal-side of him within `ind03_last_ditch_s`) is challenged whatever the odds, since the alternative is being beaten without a duel; not when a teammate covers him (DEF-02, within `box03_cover_radius` and goal-side of the carrier), who takes the carrier instead. A ball under an opponent's control (including between dribble touches) can only be won in this duel, never by simply touching it; a touch that runs closer to the defender than to the carrier raises his win probability, and one clearly closer to him (`duel.tackle_loose_margin`) is contested whatever the odds: the gate is about diving in on a ball the attacker has, not about a loose one.
- **Parameters:** `ind03_tackle_min_p` = 0.65 (calibrated; at equal skill the defender contains rather than dives in); `ind03_last_ditch_s` = 0.3 s and `duel.tackle_loose_margin` = 0.3 m (calibrated against the lab's tackle count).

### IND-04 · Marking distance
Status: v1 · Phase: out of possession
- **Source says:** "Position yourself goal-side of the ball and the attacker"; "when the opposition players are near the goal, players should mark closer"; adjust to see ball and attacker [FA-EFL]. "1v1 marking requires short distances" [FIFA-PR].
- **Numbers:** not specified.
- **Translation (ours):** marking point on the line from the marked opponent to the own goal centre, at distance `d = clamp(ind04_distance_per_m · distance_to_goal, ind04_min, ind04_max)`, shifted slightly towards the ball side. The marker moves with his man between re-reads (his velocity is the target velocity), instead of running to where the man stood at the last read; the same holds for tracking a runner (DEF-04).
- **Parameters:** `ind04_distance_per_m` = 0.08 (calibrated); `ind04_min` = 1.0 m; `ind04_max` = 5.0 m (calibrated).

### IND-05 · Defending when outnumbered
Status: v1 · Phase: out of possession
- **Source says:** "Angle their pressure to turn the 1v2 into a 1v1"; "retreat diagonally in order to cut off passing lines between the two attackers and cover the goal" [FIFA-1V2]; in 3v2 "Angle the press to cut off the third attacker" [FIFA-ZD].
- **Numbers:** none.
- **Translation (ours):** outside shooting range (BOX-01), a first defender is outnumbered when no team-mate is within `ind05_cover_m` of the carrier and an attacker nobody marks (no defender within 3 m) is within `ind05_support_m` of him. He then contains at no less than `prs07_delay_distance` from the carrier, on the bisector of the directions to the own goal and to the free attacker nearest the goal, which cuts the pass between the two and covers the goal while help comes. Not in a touchline trap (PRS-05). The 3v2 case ("angle the press to cut off the third attacker" [FIFA-ZD]) has no rule of its own beyond PRS-03's curved approach.
- **Parameters:** `ind05_cover_m` = 10 m (calibrated); `ind05_support_m` = 15 m (calibrated).

### IND-06 · Recovery runs
Status: v1 · Phase: out of possession / defensive transition
- **Source says:** "Work out the shortest route back"; "get between the ball and the goal as quickly as possible"; central defenders "pick up positions within the width of the goal" [FA-EFL].
- **Numbers:** goal width (7.32 m).
- **Translation (ours):** a player who is ahead of the ball (not goal-side) when his team is out of possession sprints to the point on the ball–own-goal-centre line at his anchor's depth; centre-backs clamp |y| to the goal half-width.
- **Parameters:** `ind06_goal_half_width` = 3.66 m (sourced, goal width).

### IND-07 · Attack the high ball
Status: v1 · Phase: out of possession
- **Source says:**
  - Heading: "Eye on the ball – read flight of ball."; "Body in line of flight."; "Timing of jump to attack ball at highest point – just before your opponent." [USS-D p.73, §6.8 "Heading & Crossing"].
  - Defending the long ball, coaching focus for the defending team: "Marking/duelling (1st ball)", "Positioning/timing (2nd ball)" [FFA-NC ch. 5 p.252, "Defending/recapturing Model Session 2 (long ball - 2nd ball)"].
  - "Yasser always marked Rondón tight, whereas Rabia would cover the space behind his team-mate." [FIFA-AHLY]
- **Numbers:** none.
- **Translation (ours):**
  - Against an opponent's lofted pass to a receiver, the outfield player who can be within `aerial_contest_radius` of the receiver's reception point soonest goes for it if he gets there no later than `ind07_challenge_slack_s` after the receiver. He runs straight to that point, without stopping at a contain distance, and duels for the first ball; when he is later, he puts the receiver under pressure as he brings the ball down. A player who reaches the ball's path first (PRS-02) plays it there.
  - The receiver attacks a contested high ball too (ATT-24). A player within `aerial_contest_radius` of a header makes it an aerial duel.
  - The header itself (2026-10-29, "direct play" round) is a headed pass: not at goal, the winner heads to the best-placed team-mate within his heading range (one with ≥ 4 m of space, the freer and the nearer to 15 m the better, forward preferred), and only with nobody to head to is it a clearance long and wide. Its receiver and the opponents react to it like any pass (`Simulation::header`, `EventKind::Pass { headed: true }`). Before, every header was knocked blindly 15–30 m forward, so a defender who won the ball in the air gave it straight back: after a lost aerial the kicking team was next in control 67–83 % of the time, after an opponent's free header ≈ 65 %; StatsBomb (40 matches, high passes of 30–45 m) 24 % and 11 % for an incomplete one overall; now 0–3 %.
  - A header won in a duel is not always a controlled one (2026-10-29, "headed-pass error" round): a share `duel.contested_header_loose` × (1 − heading) of them (0.9: 29 % for a 68-rated header) is headed away loose, long and wide, instead of to a team-mate, and a contested header's direction error is 1.5× a free one's. StatsBomb open play (40 matches, `sb_aerial_out.py`): after a lost aerial duel the winner's first action is a pass 53 %, a headed clearance 29 %, a shot 10 %; a headed clearance is next controlled by the clearing side 20 %, by the other side 66 %; an incomplete headed pass by the other side 81 %; headed passes complete 61 % (0–10 m 67 %, 10–20 m 59 %, 20–35 m 51 %; `sb_headers.py`), the engine's 58–61 % by the same bands. Before, every won header was an aimed pass, so the kicking side was next in control after a lost aerial 0–3 % of the time (StatsBomb 24 %).
  - The on-ball AI prices a contested high ball the way the sim settles it (`analysis::race`): above control height a header the receiver contests, as first man or within `aerial_contest_radius` of the opponent who is, is an aerial duel (`aerial_win_prob`) followed by a loose knock-down his side keeps `ai.aerial_won_keep` (0.9) / `ai.aerial_lost_keep` (0.05) of the time, both measured with `pass_cal`; a knock-down is worth `ai.aerial_value_factor` (0.5) of the ball received at the target. Before, the race counted an opponent who got to the ball a fraction earlier as a loss, so lofted passes priced below 0.5 were completed 52 % (predicted 31 %); now predicted and completed agree within ≈ 5 points in every band.
  - Not translated: positioning for the second ball. No federation source found says where to stand for it or who picks it up (deferred). The cover behind the duel is the existing DEF-02 cover.
- **Parameters:** `ind07_challenge_slack_s` = 1.0 s (calibrated: StatsBomb 360, 166 matches, forward high passes of 30–45 m by the nearest opponent at release; without the rule, the opponent at the completed receipt was a median 2.8 / 3.7 m away in the 0–5 / 5–10 m bands vs 1.0 / 2.1 m real; with it 1.3 / 2.5 m). `duel.aerial_contest_radius` = 1.6 m (calibrated: aerial duel at the end of these passes 11–14 % vs 14–15 % real).
- **Instruction:** per team "attack every high ball" / "only when first to it" (`TeamOption::AttackEveryHighBall` / `OnlyWhenFirstToIt`): `DefendingInstructions::challenge_slack`.

### IND-08 · Read the passer
Status: v1 · Phase: out of possession
- **Source says:**
  - "Teams should watch their opposition's body shape. This is especially true if a player is about to throw the ball – which is a great time to intercept."; key objective: "recognise when, where, and how to intercept" [FA-EFL, "Intercepting session: intercept, break and score"].
  - "When intercepting, judge whether it's possible by trying to see path of the ball and nearby attackers." [FA-EFL, "Marking and intercepting sessions: be on guard to intercept"].
- **Numbers:** none.
- **Translation (ours):** a pass is read from the passer's preparation (plant foot, backswing, body shape), so every player's reaction to a pass (any kick but a shot) ends `pass_cue_s` sooner after the release, never before it (`reach::pass_reaction`). The chaser race (PRS-02, IND-07), the AI's interception odds for its own passes (`evaluate_pass`, lane openness, ATT-06 pressure on the receiver) and the passer's next decision all use it. A defender beside the line near the passer can then still cut out a pass he could not reach reacting only to the strike.
- **Parameters:** `player.pass_cue_s` = 0.2 s (calibrated: StatsBomb 360, 166 matches, completed forward ground passes of 10–20 / 20–30 / 30–45 m with an opponent < 2 m from the pass line at release, 18 / 11 / 7 % real; without the cue the engine completed 22–25 %, with 0.2 s 14–15 %; the defenders it left out stood near the passer, 0.10–0.33 of the way along the line).
- **Instruction:** none. It is perception, not a style; how readily a player gambles on an interception is DEF-04 / IND-03 territory.

### CRS-01 · Defending crosses
Status: v1 · Phase: out of possession
- **Source says:** the closest player presses the crosser; defenders "win the first contact"; clearances for height, width, distance [FA-EFL]. Goalkeeper's "starting position … at his front post is crucial" against cut-backs, while the defenders cover the penalty-spot area [FIFA-GK]. (A claim that the full-back tracks his runner into the box was not found in [FIFA-PR] and was removed.)
- **Numbers:** none for the line depth.
- **Translation (ours):** with the ball in the own defensive third outside the width of the box, the line drops to no more than `crs01_line_depth` from the goal line, whatever the pressure on the ball (BLK-02 would hold it), so the defenders cover the penalty-spot area. The closest player presses the crosser as first defender (DEF-01), and at the target the first contact is the aerial duel (`aerial_win_prob`). Instruction `DefendingInstructions::cross_line_depth` (`TeamOption::DefendCrossesDeep` / `DefendCrossesHigh`).
- **Parameters:** `crs01_line_depth` = 14 m (calibrated: between the penalty spot at 11 m and the box edge at 16.5 m).
- **Bench:** defenders in the box at the release 2.0, against 4.54 real (StatsBomb 360): when the cross comes, the defenders are still 5–9 m short of their line target, often after a transition (movement lag, open).

## Out of possession — box defending

Near the own goal, delaying is no longer enough: FA-DNA calls this "emergency defending … a method of protecting the goal when it is at its most vulnerable and when all other defensive options have failed", whose skills for outfield players "include blocking and intercepting shots, crosses and forward passes, defending one versus one, reacting to rebounds and making clearances" [FA-DNA]. "In shooting range" below means an opponent carrier within `box01_range` of the own goal centre.

### BOX-01 · Deny the shot
Status: v1 · Phase: out of possession (low block)
- **Source says:** "engage with an attacker on the edge of the penalty area and force them wide, rather than allowing them to come inside"; the task is "preventing their opponent getting a shot off as they retreat towards their own goal"; "defenders should be set and balanced when engaging the attacker" [FIFA-1V1]. "As soon as the attacker receives the ball, the defender should be within 2-3 metres of them"; "Force attackers onto their weaker side or the outside by pressing on the inside and getting tight to them" [FIFA-SA15].
- **Numbers:** 2–3 m on the attacker's reception [FIFA-SA15] (a finishing drill, not a box-specific rule).
- **Translation (ours):** in shooting range the first defender contains at `box01_contain_distance` instead of IND-01's distance, and blocks the inside whatever the team's DEF-07 channel (show outside, away from goal). A carrier setting up a shot (`Intent::SetUp` with a shot plan) is challenged by a defender in tackle reach whatever the duel odds (IND-03's "about to shoot").
- **Parameters:** `box01_range` = 22 m (calibrated: penalty area depth plus about 5 m); `box01_contain_distance` = 1.2 m (calibrated; "getting tight", inside the 2–3 m engagement distance).

### BOX-02 · The cover blocks the shot
Status: v1 · Phase: out of possession (low block)
- **Source says:** "the two centre-backs stuck to their principles of one staying tight and the other covering his partner and blocking the shot" [FIFA-AHLY]. "Move together to remain compact goalside and narrow the shooting angle" [FIFA-LB].
- **Numbers:** none.
- **Translation (ours):** in shooting range the second defender (DEF-01/02) is the one quickest to the shooting line, and stands on the line from the ball to the own goal centre, `box02_block_distance` from the ball, moving with the carrier. Whether a shot that reaches him is blocked is body mechanics (design.md, "Shot blocks").
- **Parameters:** `box02_block_distance` = 3.5 m (calibrated).

### BOX-03 · Engage when covered
Status: v1 · Phase: out of possession (low block)
- **Source says:** "Having one defender engage the attacker in individual duels and the other cover the space behind is an effective way to mitigate the striker's threat"; "Kamal would ensure at least one spare player was covering the space, helping the central defender and essentially making it a 2v1 situation" [FIFA-AHLY]. The limit: "Be patient and do not dive in. The player who presses the ball carrier must slow down upon approach so that they do not get easily beaten" [FIFA-LB].
- **Numbers:** none.
- **Translation (ours):** in shooting range, a defender in tackle reach with a teammate goal-side of the carrier within `box03_cover_radius` tackles at a duel win probability of at least `box03_tackle_min_p` instead of IND-03's `ind03_tackle_min_p`. The approach is unchanged (PRS-03 deceleration), so he still arrives balanced.
- **Parameters:** `box03_cover_radius` = 5 m (calibrated); `box03_tackle_min_p` = 0.45 (calibrated; at equal skill a covered defender engages, an uncovered one contains).

### BOX-04 · Low-block shape in front of the box
Status: v1 · Phase: out of possession (low block)
- **Source says:** "The 4 defenders begin the exercise positioned in front of the penalty area. The 4 midfielders position themselves about 6 metres in front of the defence"; "Players within the same line must shuffle across together to keep the centre of the pitch tight and protect the goal. When the ball is on one side of the pitch, the full-back furthest from the ball must tuck in centrally to stay compact with the 2 centre-backs, in order to be able to defend a cross and a switch of play"; "Move together to remain compact goalside and narrow the shooting angle" [FIFA-LB]. "If play moves centrally, the players in the back 4 should try to stay compact and limit the distances between one another, whilst only marking wide areas when the ball is played out wide"; "The defenders should try to ensure that there is never any more than 6-8 metres between them"; "Once the attackers set foot inside the penalty area, the defenders need to ensure that they are marked at all times" [FIFA-B4]. The block moves at the speed of the ball [FIFA-MB, see DEF-06].
- **Numbers:** 6–8 m between neighbouring back-four defenders [FIFA-B4]; midfield line about 6 m in front of the defence [FIFA-LB].
- **Translation (ours):** with the ball within `box04_far` of the own goal centre, the DEF-05 block blends (fully at `box01_range`) into a low-block shape:
  - narrower, until neighbouring back-line slots are at most `box04_max_gap` apart;
  - shifted less to the ball side (`box04_shift_factor` instead of `def06_shift_factor`), so the far full-back tucks in beside the centre-backs and the line stays in front of goal;
  - shorter, until the formation's midfield line (mean depth of the DM/CM/AM/W slots) is `box04_mid_distance` in front of the back line.

  In that zone the block's lines may drop towards goal as fast as the ball travels towards it, instead of DEF-05's `def05_shift_speed`. A back-line player whose point drops runs there at least at `box04_drop_urgency`; he does not settle and jog.
- **Parameters:**
  - `box04_max_gap` = 6 m (sourced, lower end of FIFA-B4's 6–8 m, because players stepping out to press or mark leave real gaps wider than the anchors' gaps);
  - `box04_mid_distance` = 6 m (sourced, FIFA-LB);
  - `box04_far` = 45 m (calibrated against StatsBomb back-four widths at 30–40 m, see `docs/design.md`);
  - `box04_shift_factor` = 0.4 (calibrated);
  - `box04_drop_urgency` = 0.7 (calibrated).

## Defensive transition

### TRN-01 · Counter-press or regroup
Status: v1 · Phase: defensive transition
- **Source says:** favourable for counter-pressing: (1) ball lost in the front third, (2) equal or superior numbers near the ball, (3) good staggering in depth and width, (4) ball carrier under pressure, (5) ball lost on the wing, (6) opponent sitting very deep. Drop instead if the team was spread out or no pressure on the ball is possible [BDFL-KRUE]. "The closest player to the ball must immediately react … and press"; if not possible, "close off potential passing lanes, secure central areas" [FIFA-TD].
- **Numbers:** six criteria.
- **Translation (ours):** at ball loss each player's brain scores the six criteria (each 0/1) and the team counter-presses if the score reaches `trn01_min_criteria`; otherwise it switches to recovery (TRN-03).
- **Parameters:** `trn01_min_criteria` = 3 (calibrated); `trn01_near_radius` = 15 m (calibrated, "near the ball").

### TRN-02 · Counter-press duration
Status: v1 · Phase: defensive transition
- **Source says:** "about five seconds" (transl. of "erfahrungsgemäß etwa fünf Sekunden") to win the ball back [BDFL-KRUE]; win it back "within five seconds" [FA-EFL] (framed there as a training challenge, not a rule); UEFA analyses use a five-second window [UEFA-PI].
- **Numbers:** 5 s.
- **Translation (ours):** counter-press ends after the window; then recovery / block.
- **Parameters:** `trn02_window_s` = 5.0 s (sourced).

### TRN-03 · Recovery and squeeze
Status: v1 · Phase: defensive transition
- **Source says:** on losing the ball "the backline should squeeze tight"; minimise space between and behind the lines; players far from or behind the ball run back [FIFA-TD]. Delay by forcing wide [FA-EFL]. "PRESSURE the opponent 'later' by dropping back first" [FFA-433].
- **Numbers:** none.
- **Translation (ours):** in recovery the block length is multiplied by `trn03_squeeze_factor`, players ahead of the ball make recovery runs (IND-06), and the first defender only delays (PRS-07).
- **Parameters:** `trn03_squeeze_factor` = 0.85 (calibrated).

### TRN-04 · Tactical foul
Status: v1 · Phase: defensive transition
- **Source says:** UEFA's EURO 2012 report lists "the use of 'tactical fouls' to break up counters" among the "counter-the-counter ploys" [UEFA-E12]. "Counter-attacks are also often stopped with a foul close to the penalty box, or go out for a corner" (TSG members, WC 2018) [FIFA-NEWS18]. The Law cautions a player who "commits any other offence which interferes with or stops a promising attack" [IFAB-L12]. A data firm's operational rule: "we selected those fouls that occur within the first five seconds after ball loss and within the last 60 meters of the field" [DATA-TF01].
- **Numbers:** Premier League teams 45 (Manchester City) to 79 (Liverpool) such fouls by the end of February 2024 [DATA-TF01], ≈ 1.7–3 per match.
- **Translation (ours):** a player beaten in a challenge within `trn04_window_s` of his team losing the ball, in the last `trn04_zone_m` of the pitch towards the opponents' goal, fouls the carrier with probability `trn04_foul_p` instead of LAW-02's `foul_beaten` (× (1 ± `foul_aggression`)). The foul goes through the referee like any other (LAW-02: usually an SPA caution). A cautioned player commits none (layer 3, LAW-02 `caution_care`).
- **Parameters:** `trn04_window_s` = 5 s, `trn04_zone_m` = 60 m (sourced [DATA-TF01]); `trn04_foul_p` = 0.22 (calibrated against the DATA-TF01 counts).
- **Instruction:** per team "stop counters with a foul" / "no tactical fouls" (`TeamOption::StopCountersWithFouls` / `NoTacticalFouls`): `DefendingInstructions::tactical_foul`.
- **Bench:** (round h of M7, `match_lab` 40 matches) 2.9 tactical fouls per team-match, against ≈ 1.7–3 per match [DATA-TF01] (× 1.62 for the time in play ≈ 2.7–4.8).

## Goalkeeper

### GK-01 · Angle play
Status: v1 · Phase: out of possession
- **Source says:** "Cover the angle of the shot"; shoulders facing the ball; "keeping an eye on the posts" [FIFA-GK, Santangelo "Defending the goal: diving"]. "Close the angle swiftly by moving towards the ball" [FIFA-GK, U15 Spain].
- **Numbers:** none (no bisector rule found in federation material).
- **Translation (ours):** the goalkeeper stands on the line from the ball to the goal centre (approximation of the angle bisector) at the depth from GK-03. Within shooting range he closes the angle by moving towards the ball (`analysis::gk01_depth`): depth `gk01_narrow_depth` off the line with the ball at the penalty spot, plus `gk01_depth_per_m` per metre further out, at most `gk01_max_depth` and never more than `gk01_depth_frac` of the way to the ball. Beyond `gk03_shot_range` this blends into the GK-03 sweeper depth over 10 m, so he is set by the time the ball is in range.
- **Parameters (all calibrated):**
  - `gk01_narrow_depth` = 1.8 m, `gk01_depth_per_m` = 0.12, `gk01_max_depth` = 2.5 m, `gk01_depth_frac` = 0.3.
  - Fitted to the keeper's position in the freeze frames of [StatsBomb open data](https://github.com/statsbomb/open-data): 610 matches, open-play shots.
  - Real median depth: 1.2 m for shots from 0–6 m, 1.6 m at 9–12 m, 2.1–2.3 m at 12–25 m and 1.8–2.0 m at 25–35 m.
  - At 20–35 m only ≈ 1 % of keepers are more than 6 m off their line.
  - The earlier values (3.0 m at the spot, up to 40 % of the way) put engine keepers 5–9 m out at 16.5–40 m shots.

### GK-02 · Depth and smothering
Status: v1 · Phase: out of possession
- **Source says:** "If the goalkeeper cannot get within 3 metres of them, they should stay in goal and narrow the angle"; smother at 0–3 m; tall posture against long-range shots; take "a high position (close to the edge of the penalty area)" and retreat as the shooter draws back; too high invites a lob [FIFA-GK].
- **Numbers:** 3 m.
- **Translation (ours):** if a ball carrier is through on goal (no defender between him and the goalkeeper, GK-03) and the goalkeeper can get within `gk02_smother_distance` of him before he can shoot, the goalkeeper rushes out; otherwise he narrows the angle (GK-01/03).
- **Parameters:** `gk02_smother_distance` = 3.0 m (sourced).

### GK-03 · Sweeper-keeper depth
Status: v1 · Phase: out of possession
- **Source says:** WC2022 average goalkeeper position 13.1 m from the goal line and 20.4 m from his defensive line; when winning, the distance to the defensive line decreased by 1.5 m on average compared to 2018; when losing, the average height was 14.2 m (12.8 m in 2018). "There is no absolute right position"; the key question is "Is there a defender between the goalkeeper and the attacking player with the ball?" [FIFA-GKH]. The goalkeeper acts as a sweeper ("libero") [FFA-NC; FFA-433] and starts high in a high press [FIFA-HP].
- **Numbers:** as quoted.
- **Translation (ours):** goalkeeper depth = defensive line x − `gk03_line_gap`, clamped between `gk03_min_depth` and `gk03_max_depth` from the goal line. When the ball is within `gk03_shot_range` the keeper switches to the angle-narrowing depth of GK-01 (GK-02: "stay in goal and narrow the angle"). This holds in possession too (BU-01), so a turnover in shooting range does not find him high. In StatsBomb, keepers average 2.2 m off their line at shots from 16.5–40 m that come within 5 s of a possession starting; only 3.8 % of them are more than 5.5 m out.
- **Parameters:** `gk03_line_gap` = 20.4 m (sourced, average); `gk03_min_depth` = 1.0 m; `gk03_max_depth` = 18.0 m (calibrated); `gk03_shot_range` = 40.0 m (calibrated).

### GK-04 · Crosses and command of area
Status: v1 · Phase: out of possession
- **Source says:** "Don't step out too early and before the cross is played"; read near / back post; "Command the space" [FIFA-GK].
- **Numbers:** keeper first on 2.3 % of open-play crosses, 3.1 % of high ones (StatsBomb 360 open data, 166 matches).
- **Translation (ours):** a cross is priced with the keeper racing only its flight time (FT-02: he comes once it is played). Among his team's chasers he goes for an opponent's pass into his penalty area only if he arrives `gk04_claim_margin_s` × (1 − aerial command) before its receiver; otherwise the best outfield interceptor goes (PRS-02). His starting position is GK-01's angle position, which puts him at the near post against a ball from wide [FIFA-GK]. Reading near / back post has no rule of its own.
- **Parameters:** `gk04_claim_margin_s` = 0.6 s (calibrated).
- **Bench:** keeper first 6.6 % of crosses: 4.0 % of lofted ones (real 3.1 %) and 10.3 % of ground ones (real 0.9 %), because the engine plays too many ground crosses from deep (FT-02, open).

---

## In possession — ball carrier

### ATT-01 · Penetrate first
Status: v1 · Phase: in possession
- **Source says:** "The first choice of the player with the ball is to penetrate"; "first option to penetrate by: 1) shot, 2) pass or 3) dribble?" [USS-D p.41, 53].
- **Numbers:** order only.
- **Translation (ours):** in the on-ball utility, a small tie-break bonus in the order shot > forward pass > forward dribble, so that when values are close the penetrating option wins.
- **Parameters:** `att01_shot_bonus` = 0.1; `att01_forward_pass_bonus` = 0.1; `att01_forward_dribble_bonus` = 0.05 (calibrated; the pass and dribble tie-breaks are fractions of the local value scale, i.e. the possession value at the ball, the shot's a fraction of its own expected goals).
  - The shot's tie-break (2026-10-30): as a share of the local value scale it was ≈ 0.006 at 25 m, where that scale is two to three times a long shot's xG, so it was 30 % of a 25 m shot's worth and no tie-break. A share of the shot's own xG keeps the order (shot first when the values are close) without inflating poor shots (`shot_share`: 22–30 m shot share 31.7 → 28.7 %, 16.5–22 m 54.9 → 47.1 %; real 7.9 / 23.9 %).

### ATT-02 · Risk increases towards goal
Status: v1 · Phase: in possession
- **Source says:** "Je näher zum eigenen Tor desto mehr steht die Ballsicherung im Vordergrund … im Angriffsabschluss Mut zum Risiko" (the closer to the own goal, the more ball security matters; in the finish, courage to take risks) [DFB-FLICK]. US Soccer tactics by third: defensive third "DENY - DESTROY - DEVELOP", midfield "BUILD - CONNECT - SUPPORT", attacking "MOVE - RECEIVE - FINISH" [USS-D p.45–46].
- **Numbers:** thirds.
- **Translation (ours):** the turnover cost term C is weighted by the third in which the ball would be lost. Ball security in the own box is absolute: the carrier never dribbles back towards the own goal inside the own penalty area (prevents own goals from heavy touches).
- **Parameters:** `att02_risk_weight_own_third` = 1.4; `att02_risk_weight_middle_third` = 1.0; `att02_risk_weight_final_third` = 0.6 (calibrated).

### ATT-03 · Furthest comfortable pass
Status: v1 · Phase: build-up / progression
- **Source says:** "the unit must decide on the furthest and most comfortable pass that they can play" [FIFA-TCP, "playing forward as a priority"].
- **Numbers:** none.
- **Translation (ours):** among pass candidates whose success probability is at least `att03_comfortable_p`, a bonus proportional to forward gain.
- **Parameters:** `att03_comfortable_p` = 0.8 (calibrated); `att03_forward_bonus_per_m` = 0.005 (calibrated, fraction of the local value scale per metre).

### ATT-04 · Bind defenders when nothing is on
Status: v1 · Phase: in possession
- **Source says:** dribble to "Gegenspieler zu binden statt freie Räume anzulaufen" (tie opponents down instead of running into free space) [DFB-FLICK]; with no options and no pressure: "Kurz andribbeln, den Gegner zu einer Aktion zwingen" (dribble at him briefly, force the opponent to act) [DFB-SCHOEN p.21].
- **Numbers:** none.
- **Translation (ours):** when no pass has success probability ≥ `att03_comfortable_p` and the carrier is not pressured, the dribble candidate towards the nearest opponent gets a bonus.
- **Parameters:** `att04_bind_bonus` = 0.15 (calibrated, fraction of the local value scale).

### ATT-05 · Scanning and receiving with an open body
Status: v1 · Phase: in possession
- **Source says:** "scan before receiving"; "offer for the ball at an angle to be able to receive on the back foot"; "adopt an open body shape … receive on the half-turn"; "take their first touch forward and away from pressure" [FIFA-TCP; FIFA-ES Zamrat; USS-D p.69–70]; receiving "in einer geschlossenen Stellung" (closed stance) is an error [DFB-SCHOEN p.19].
- **Numbers:** none.
- **Translation (ours):**
  - A receiver who will play first time (ATT-06) opens his body half-way between the incoming ball and where he will play it. A first-time kick is only an option if it lies within the kick's set-up angle (`execution::setup_angle`) plus `first_time_open_deg` of the direction the ball comes from; anything further round needs a controlling touch and a turn first.
  - Look again: the receiver takes his first-time decision (ATT-06) when the pass is released and once more when his reaction time has passed, with the same decision seed (`Simulation::decide_reception`). A defender who closes in, or one who peels away, can change his mind between a first-time pass and a controlling touch.
  - Scanning (M6, `ai::scan`): every player looks away from the ball `scan_rate_min`…`scan_rate_max` times per second by intelligence, × `scan_pressed_share` with an opponent within 1 m, rising to the full rate at 4 m [JORDET20]. A scan takes a snapshot of the opponents. Between scans he sees only those within `scan_view_half_angle` of his facing; anyone outside is where the last snapshot put him, carried on at the velocity it saw for at most `scan_extrapolate_s`. What he perceives decides his judgement of pressure (ATT-06), the direction of his controlling touch (away from the nearest opponent he perceives when pressed) and the passer's offside judgement (LAW-01); a team-mate who sees pressure the receiver does not can shout "man on" (calls, layer 3), which refreshes his picture. After controlling the ball he settles × (1 − `scan_settle_gain`) faster with a scan within `scan_fresh_s` and nobody close, × (1 + `scan_settle_gain`) slower with none within `scan_stale_s` [JORDET13].
  - In the engine, pressed receivers who control lose the ball more often than real ones, so the AI escapes pressure with first-time passes more often than real players (see docs/design.md "Short-pass losses and conditional targets").
- **Numbers:** 0.44 ± 0.30 scans/s in the 10 s before receiving, central midfielders most, forwards least, fewest with an opponent within 1 m; a small positive effect on completion (log-odds 0.13–0.16 per SD) [JORDET20]. Pass completion 64 / 68 / 81 % after low / medium / high scan frequency [JORDET13].
- **Parameters:** `first_time_open_deg` = 60° (calibrated); `scan_rate_min` = 0.1, `scan_rate_max` = 0.8 scans/s, `scan_pressed_share` = 0.4 (calibrated against the 0.44 mean [JORDET20]); `scan_view_half_angle` = 100°, `scan_extrapolate_s` = 0.5 s, `scan_fresh_s` = 1.5 s, `scan_stale_s` = 4.0 s, `scan_settle_gain` = 0.3 (calibrated against [JORDET13]).
- **Bench:** 0.44 scans/s before receiving; the next pass completed after 0–2 / 3–4 / 5+ scans in the 10 s before: 82 / 85 / 87 %. The gradient is far smaller than JORDET13's 64 / 68 / 81 % (open: the perceived picture decides pressure, the touch and offside, while the pass choice still prices its lanes against the true positions).

### ATT-06 · Pressure decides the touch
Status: v1 · Phase: in possession
- **Source says:** "When receiving under pressure, players should prioritise playing first-time passes"; a player "receiving free of any mark, must try and exploit opportunities to turn" [FIFA-TCP]. FIFA pressure model: defender in front of the carrier scores higher than behind [FIFA-EFI p.12].
- **Numbers:** none.
- **Translation (ours):** a receiver is "under pressure" if an opponent can reach him within `att06_pressure_time_s` of the ball's arrival. Under pressure he decides before the ball arrives and may play first time; when free he controls and turns towards goal.
  - In the valuation (2026-10-29): the continuation a carrier is credited for keeping the ball (the best pass on offer, kept for the next decision) is only worth having while free: it is scaled by (1 − pressed at the next decision) for holding and by (1 − pressed at the end) for a dribble. Credited in full it let a pressed carrier defer the pass step after step. Measured with the `hold_cal` example (hold decisions by the pressure expected at the next decision, against what followed): per decision a pressed hold loses the ball 3–5 % of the time, over the spell of holds before the next kick 18–30 %; the best pass at the next decision is no worse than now, so holding is not wrong per step, which is why this change moves little in the own half (the continuation there is ≈ 0.0002 on values of ≈ 0.01) and more in the final third, where it releases the ball earlier (lab goals 2.65 → 3.3).
  - Stale plans (2026-11-01, watch round): the first-time action is priced for the reception point at the time of the decision, but a pressed receiver who then meets the ball earlier (ATT-24) played it wherever he touched it. 43 % of first-time shots were struck 8 m or more from where their xG was priced, among them 40 m shots priced as 25 m ones. He now re-decides on the spot when he meets the ball more than `att06_replan_m` from the planned point; afterwards every first-time shot was struck within 1 m of it.
  - Priced at arrival (2026-11-01): a first-time shot's xG is computed against where the keeper will be when the ball arrives (`Situation::arrives_in` added to the xG's keeper lead), not where he stands at the pass.
- **Parameters:** `att06_pressure_time_s` = 0.6 s, `att06_replan_m` = 2 m (calibrated).

### ATT-07 · Pass length categories
Status: v1 · Phase: in possession
- **Source says:** long "30 metres or more"; medium "between 10 and 30 metres"; short "less than 10 metres" [FFA-NC slide 14].
- **Numbers:** as quoted.
- **Translation (ours):** pass kind selection: short passes are ground passes, medium ground or driven, long may be lofted; also used for statistics. A lofted pass is launched at `ai.lofted_launch_angle` (not from the source: calibrated so that 30–45 m lofted passes take the StatsBomb median 2.45–2.56 s from kick to first contact; see design.md "Lofted volume").
- **Parameters:** `att07_short_max` = 10.0 m (sourced); `att07_long_min` = 30.0 m (sourced); `ai.lofted_launch_angle` = 0.65 rad (calibrated).

## In possession — support and structure

### ATT-08 · Support angle and distance
Status: v1 · Phase: in possession
- **Source says:** options "forward, square or back"; "good angles of support, proper distance of support and speed of support"; triangles, straight lines destroy support. A supporting player is "Positioned at the proper distance to insure that one defender cannot restrict both attackers"; "behind, square or in advance of the ball based upon pressure" [USS-D p.41–42, 53].
- **Numbers:** not specified (geometric rule only).
- **Translation (ours):** **show** is a movement of every role (docs/player-brain.md sections 6 and 8) to a point on a ring round the carrier (10 m, 16 m and his support distance clamped to `att08_min_distance`–`att08_max_distance`; on the line from the carrier to his spot and 0.35 / 0.7 rad either side of it) that he reaches within `show_reach_s`. It is valued by its offer (ATT-26: the race along the lane and to the point, so a lane one defender can cut and a point he can reach first price themselves: "one defender cannot restrict both"), its marginal value to the carrier and the space it leaves; he gets there by the carrier's next decision.
- **Parameters:** `att08_min_distance` = 6.0 m; `att08_max_distance` = 25.0 m (calibrated); `show_reach_s` = 2.0 s (calibrated).
- **History:** until phase G (2026-10-31) a positioning score term rewarded open lanes (`att08_lane_weight`).

### ATT-09 · At least three options
Status: v1 · Phase: in possession
- **Source says:** "always create at least 3 options for the player on the ball"; "Triangles (no players in straight lines)" [FFA-GTC p.10]; "Provide 4 options (left; right; central and far)" [FFA-GTC p.27, drill coaching point].
- **Numbers:** ≥ 3 options.
- **Translation (ours):** the count is not a rule but follows from the marginal value: an offer adds most while the carrier has few good options (the softmax inclusive value of docs/player-brain.md section 8.3). The carrier's options also change before his next decision, so backup options keep a value (`offer_uncertainty` widens the temperature of the expected best). Test 7: the centre-back in the build-up has at least `att09_min_options` safe passes, on both sides.
- **Parameters:** `att09_min_options` = 3 (sourced; the test-7 threshold); `offer_uncertainty` = 0.5 of the local value scale (calibrated).
- **History:** until phase G each brain counted the open options and boosted the support term below three (`att09_boost`).

### ATT-10 · Staggered lines
Status: v1 · Phase: in possession
- **Source says:** "When players are aligned vertically, it reduces the passing angles"; central midfielders "should not be positioned on the same vertical or horizontal line"; "avoid offering the same supporting movements, crowding the space" [FIFA-TCP].
- **Numbers:** none.
- **Translation (ours):** no penalty term. An offer behind a team-mate who stands within `SCREEN_M` = 1.5 m of the lane cannot be played (`ai::offer`); two offers within `offer_overlap_m` are one option (docs/player-brain.md section 8.3), so a second player in line adds nothing and goes elsewhere; a player following his movement takes the candidate nearest his last offer that is not within `offer_overlap_m` of a team-mate's. The ATT-10 scenario test checks that no two supporters stand collinear with the carrier.
- **Parameters:** `att10_collinear_deg` = 12.0° (calibrated; the test threshold); `offer_overlap_m` (ATT-26).
- **History:** until phase G collinear, same-line and crowding penalties (`att10_collinear_penalty`, `att10_same_line_tol`, `att10_crowding_radius`).

### ATT-11 · Width and depth
Status: v1 · Phase: in possession
- **Source says:** "use all the width of the field and as much of the length as the offside rule will allow" [USS-D p.41]. "Raum so tief wie möglich und so breit wie nötig aufteilen" (occupy space as deep as possible and as wide as necessary) [DFB-FLICK]. Wingers' "starting position should always be high and wide" [FFA-NC slide 45].
- **Numbers:** depth bounded by the offside line. WC2022 means (64 matches, FIFA post-match reports), length × width, back line from own goal: build-up in the own third 39.2–39.5 × 53.5–54.5 m, line 19.5–19.7 m; build-up in the middle third 32.8–33.2 × 55.0–55.5 m, line 40.0–40.6 m; final third 35.8–36.6 × 43.3–45.2 m, line 52.6–53.7 m [SPORMETRE-WC22].
- **Translation (ours):** the in-possession shape stretches from a back line set by the ball's third to the opponents' offside line; wide slots use the block width, which also depends on the ball's third. Both are given at the centre of each third and interpolated linearly in between (held beyond the outer centres). The block centre follows the ball sideways (0.15 · ball y) but keeps the wide slots `att11_touchline_margin` inside the touchline.
- **Parameters:** `att11_line` = [19.6, 40.3, 53.3] m from the own goal (sourced, WC2022 means for own / middle / final third; manager-settable as `AttackingInstructions.line`, `TeamOption::PushUpBehindTheBall` / `StayDeep` ± 8 m); `att11_width` = [54.1, 55.3, 44.0] m (sourced, same table; scaled by the team width instruction); `att11_onside_margin` = 0.7 m (calibrated); `att11_touchline_margin` = 2.5 m (calibrated). Emergent (lab, 40 matches): 36.8 × 44.4 m, line 17.9 m in build-up; 34.5 × 51.9 m, line 35.0 m in progression; 39.9 × 49.6 m, line 46.2 m in the final third. The lines fall short because the opponents' blocks sit deep (see the calibration targets) and pin the offside line; raising the knots to [22, 47.5, 61.5] puts the lines on target but goals rise to 5.5 per match and possession changes to 305, so the sourced values stay until the block heights are calibrated.

### ATT-12 · Five vertical channels
Status: v1 · Phase: in possession
- **Source says:** FIFA divides the pitch into five vertical channels (left, left inside, central, right inside, right) [FIFA-EFI p.10]. Drill rule: "at least 1 player occupying each channel at all times" [FIFA-TCP] (that drill splits a 20 × 20 m area into three channels, so it supports the occupation idea, not the five-channel geometry).
- **Numbers:** 5 channels; widths not specified.
- **Translation (ours):** channel borders at the penalty-box and goal-area lines (role lanes, the block's ball-side triggers, the TR-05 ball-side release). **In to out** and **out to in** are movements 8 and 14 m across from his spot, valued by their offers; a channel gets occupied because an offer there is worth something, not by a bonus.
- **Parameters:** `att12_central_half_width` = 9.16 m; `att12_inside_half_width` = 20.16 m (calibrated: borders at goal-area and penalty-area lines).
- **History:** until phase G a bonus for a channel not yet occupied ahead of the ball (`att12_channel_bonus`).

### ATT-13 · Central occupation
Status: v1 · Phase: in possession
- **Source says:** "Create diamond and triangular structures …"; "A player should always occupy the central areas" [FIFA-ES Flores].
- **Numbers:** none.
- **Translation (ours):** every formation has centre-lane roles in each unit (`team::Lane::Centre`: centre-backs, pivots, central midfielders, centre-forwards) whose spots lie in the central channel, and hold is among their strongest priors, so the centre stays occupied unless an offer elsewhere is worth more. No bonus term; a central duty only if a scenario test shows the centre left empty (docs/player-brain.md section 13).
- **Parameters:** none.
- **History:** until phase G a central bonus (`att13_central_bonus`).

### ATT-14 · Offer in the gaps, not in cover shadows
Status: v1 · Phase: in possession
- **Source says** [DFB-SCHOEN]:
  - Hints for the central player (1.2): "Sehr gut: Der zentrale Spieler bietet sich im Rücken des Gegners für das Anspiel an" (very good: the central player offers for the pass behind the opponent); "Fehler: Nicht entgegenkommen! Der Gegner kann leicht heranrücken und den Raum schließen" (error: don't come towards the ball, the opponent can easily step up and close the space). This is about the central midfielder, not the striker.
  - Corrections (4.1): #15 "agiert nicht im Rücken des Gegners neben ihm", so he cannot be found and blocks the lane into depth; correct is "Anbieten an der Linie weiter vorne im Rücken des Gegners". Both strikers stand "im Deckungsschatten der beiden stellenden gegnerischen Mittelfeldspieler" (in the cover shadow of the two opposing midfielders who have stepped up) and "hätten so tief wie möglich gehen sollen, um die gegnerische Verteidigung zu binden und mehr Räume zu schaffen!" (should have gone as deep as possible, to pin the opposing defence and create more space), while the ball-far midfielder, covered by the ball-near opposing forward, "müsste sich auf Lücke anbieten" (should offer in the gap).
- **Numbers:** none.
- **Translation (ours):** no penalty term. An offer in a cover shadow (the defender close to the segment carrier → candidate) loses its race on the lane, because the defender in it intercepts, so a perfect reader never chooses one (the ATT-14 test and test 4 use perfect readers; an ordinary reader's layer-3 noise may still put him there). It applies to every role. The DFB-SCHOEN striker correction is situational: in test 4 both strikers in the midfielders' shadow stay on the line (ATT-26) and the ball-far midfielder shows out of the forward's shadow.
- **Parameters:** `att14_shadow_deg` = 10.0° (calibrated; the test threshold for a cover shadow).
- **History:** until phase G (2026-10-31) a penalty for candidates in a cover shadow (`att14_penalty` = 1.5). From the final-third round until ATT-26 (both 2026-10-30), role ST was exempt from that penalty, from ATT-15 and from the ATT-08 lane bonus short of his anchor. That blanket exemption misread the source and was removed.

### ATT-15 · Between the lines
Status: v1 · Phase: progression
- **Source says:** "#10 tries to get on the ball in the space between the opponent's back four and midfield" [FFA-NC slide 45]; receivers position "in space and away from opposition intervention" [FIFA-TCP]; receptions behind / between lines are tracked by FIFA [FIFA-EFI p.7].
- **Numbers:** none.
- **Translation (ours):** opponents are grouped into units (lines) by 1-D clustering of their x positions. The **drop** is a movement to the midpoint between the opponents' last two units (in his lane or 6 / 12 m beside it, when they are at least 2 × `att15_min_space` apart), valued by its offer (ATT-26).
- **Parameters:** `att15_min_space` = 3.0 m (calibrated).
- **History:** until phase G a bonus for candidates between the units with free space (`att15_between_bonus`).

### ATT-16 · Movement-to-receive types
Status: v1 · Phase: in possession
- **Source says:** five movement types relative to opposition units: in front, in between, in to out, out to in, in behind ("behind the opposition's final unit") [FIFA-FL].
- **Numbers:** none.
- **Translation (ours):** the five types are the movement repertoire itself (`team::Movement`, docs/player-brain.md section 6): in front = **show** and **check**, in between = **drop**, in to out and out to in = **in to out** and **out to in**, in behind = **run in behind**. Each role has priors for the ones it uses (`data/roles`); each is valued by its offer (ATT-26).
- **History:** until phase G the candidate generator added one point per type to the positioning scorer.
- **Parameters:** none.

### ATT-17 · Line breaks
Status: v1 · Phase: progression
- **Source says:** "An opposition line is broken when the attacking team play the ball beyond the deepest player in that line"; through, around or over; breaking the defensive line is the most valuable [FIFA-EFI p.6].
- **Numbers:** none.
- **Translation (ours):** pass utility bonus per opposition unit broken, doubled for the last unit.
- **Parameters:** `att17_line_break_bonus` = 0.12 (calibrated, fraction of the local value scale); `att17_last_line_factor` = 2.0 (calibrated).

### ATT-18 · Runs in behind
Status: v1 · Phase: progression / final third
- **Source says:** "Räume im Rücken des Gegners erkennen und nutzen" (recognise and use space behind the opponent) [DFB-FLICK]; give "multiple options in between and behind the defence … time their runs to avoid straying offside" [FIFA-TCP]; "Bending their runs" [FIFA-ES Urias]. FIFA counts receptions behind the midfield or defensive line as a metric [FIFA-EFI p.7].
- **Numbers:** none.
- **Translation (ours):** the run is one of the front-line movements of ATT-26, valued by its offer: the ball into the space behind (half to all of `att18_run_depth` beyond the line, in his lane, the channel beside it or diagonally) that the run reaches first, timed to cross the line at full speed as the pass is struck, playable only by an open carrier (as well as his passing allows). He waits onside on the line in his lane (`att11_onside_margin`; offside itself is enforced from M7), sprints when the carrier sets the pass up (ATT-24), and the carrier's through ball aims at the run's offer. With the carrier facing forward and `att18_min_time_on_ball_s` on the ball, the on-ball AI also offers through balls into the space ahead of roles that run in behind.
- **Parameters:** `att18_run_depth` = 12.0 m (calibrated); `att18_min_time_on_ball_s` = 0.8 s (calibrated).

### ATT-19 · Overloads
Status: v1 · Phase: build-up / progression
- **Source says:** "use numerical superiority in the centre of the pitch to play through pressure" [FIFA-TCP]; receiving between lines pulls defenders out of position and creates overloads [FFA-NC, paraphrase; wording not verified]; centre-backs "move into the midfield with the ball at their feet" [FIFA-ES Zamrat].
- **Numbers:** none.
- **Translation (ours):** no counter of its own. A player goes where his offer adds most to the carrier's best option (marginal value, docs/player-brain.md section 8). That is where his side outnumbers the opponents near the ball: a team-mate's offer in a space one defender cannot close as well as his own. The centre-back moves into the midfield off the ball (`Movement::StepIntoMidfield`, ROLE-02) or carries the ball there, priced like any dribble by the possession value.
- **Parameters:** none.
- **Bench:** a free centre-back in the build-up (nearest opponent 13–23 m) carries forward in 5–17 % of his choices, passing short in most of the rest. Whether that is too rarely has no real figure here yet (open).

### ATT-20 · Combinations and third man
Status: v1 · Phase: progression / final third
- **Source says:** "one-twos, up-back-throughs, overlaps and underlaps are very efficient ways to penetrate" [FIFA-ES Zamrat]; also wall pass, take-overs [USS-D p.42].
- **Numbers:** none.
- **Translation (ours):** **overlap** and **underlap** are movements of full-backs, wing-backs and wide midfielders round a team-mate on the ball out wide on their side (|y| ≥ 8 m, not more than 5 m ahead of them), outside or inside him, 6 and 12 m beyond the ball; valued by their offers and burst at full speed. One-twos, up-back-throughs and third-man passes have no movement of their own: the on-ball utility plays them when they are the best options (the third-man run's look-ahead is test 15). Overlaps stay rare without the instruction because the wide area beyond the ball is worth little until crosses and cut-backs exist (M4, FT-02).
- **Parameters:** none beyond the movement priors (ROLE-05, ROLE-06, ROLE-11); per player `overlap` instruction (layer 2).

### ATT-21 · Switching play
Status: v1 · Phase: build-up / progression
- **Source says:** "Attack down one side of the pitch to attract pressure", then switch; "Play back inside if a forward pass is not possible to avoid being closed down by the opposition towards the touchline"; play forward quickly after the switch [FIFA-ES Boumehdi; FIFA-ES Flores].
- **Numbers:** drill rule only (3–5 passes before switching). In StatsBomb open data (40 matches, ×1.48 per match) open-play passes that cross ≥ 25 m to the other half of the width number ≈ 98 per match (79 % completed; ground 48 at 97 %, high 44 at 58 %), ≥ 34 m ≈ 34 (StatsBomb's own `switch` flag gives the same 34).
- **Translation (ours):** a pass to the other half of the width that crosses at least 25 m gets a bonus once the switch exploits pressure the ball side has drawn: the carrier is pressed (`analysis::pressure_at` ≥ `att21_min_pressure`) and there are `att21_density_ratio` times as many opponents within 20 m of the ball as of the target. Without the pressure condition the bonus applied to most far-side passes, because the block is always denser on the ball side (DEF-06).
- **Parameters:** `att21_density_ratio` = 1.5 (calibrated); `att21_switch_bonus` = 0.2 (calibrated, fraction of the local value scale); `att21_min_pressure` = 0.5 (calibrated: an opponent within ≈ 3.5 m, goal-side).
- **Instruction:** per team "switch play" / "keep it on this side" (`TeamOption::SwitchPlay` / `KeepItOnThisSide`): `AttackingInstructions::switch_bonus`.

### ATT-22 · Mobility
Status: v1 · Phase: in possession
- **Source says:** movement creates space [USS-D p.41]; "Certain passes should trigger specific movements … on the move while the ball is rolling" [FIFA-ES Zamrat].
- **Numbers:** none.
- **Translation (ours):** at pass release every teammate re-evaluates his position immediately, using the predicted reception point as the ball reference.
- **Parameters:** none.

### ATT-23 · Improvisation
Status: v1 · Phase: in possession
- **Source says:** individual flair, clever dribbling and turns to create chances [USS-D p.42].
- **Numbers:** none.
- **Translation (ours):** implemented through the layer-3 personality bias of the `flair` attribute (take-ons, curled shots, through balls).
- **Parameters:** see `PersonalityConfig`.

### ATT-24 · Meet the ball
Status: v1 · Phase: in possession
- **Source says:**
  - "meeting the ball with good timing"; "the weight of the pass should enable the receiver to meet the ball, without breaking their stride"; a badly weighted pass "can be cut out by defenders or require the receiver to slow down" [FIFA-TCP, "Finishing: Offering to receive and score"].
  - Anticipation got a player "from half a metre behind to in front of her opponent in just a few steps … enabling her to reach the ball first" [FIFA-SOG].
  - "How will you move to meet the ball?"; "Are you receiving under pressure or not?"; "move into position at the right time"; "judge when the pass is ready to be played" [FA-EFL, receiving articles].
- **Numbers:** none.
- **Translation (ours):**
  - Once a pass is played, the intended receiver goes to the point on its path where he will control it. That is where it was aimed if he can be there `att24_set_margin_s` before the ball; he arrives and sets himself. Otherwise it is the earliest point of the path he can reach. For a ball along the ground he steers onto its line and stops across it, moving along the line at the speed that brings him to the point together with the ball (towards the passer when he meets it; `positioning::receive_steering`). Before, he ran at the point itself and, coming from the side, crossed the line before the ball got there and missed it. A ball dropping from the air comes down where it comes down, so there he arrives and stops when he has time to spare. Before, receivers ran through the drop point of lofted passes and the ball bounced on behind them: 9 % of lofted passes went out of play and 72 % reached the receiver (now 2 % and 80 %).
  - If he can reach no point of the path, he stops being the receiver, so the team's chaser goes for the ball. Before, he kept running to a stale point while nobody chased, and ≈ 5 % of free passes rolled out of play.
  - While the passer sets the pass up (turning his body to it), the receiver reads it and moves before the ball is struck. If he would be pressed at the target of a ground pass (ATT-06), he comes `att24_meet_m` towards the passer, at most 0.3 × the pass length, without braking. If he is free there, or the pass is lofted (coming short would let it fly over his head), he goes to the target and sets himself.
  - Reading the passer (layer 3, docs/player-brain.md section 7): he anticipates the pass as far as he understands the passer. He reads it `reading_receive_delay_max_s` × (1 − u) after it becomes legible (the start of the set-up, or the strike), a perfect reader at once, and keeps his movement until then; the passer prices the delay in the pass's race. So pairs who know each other complete more passes [LECOZ25] (familiarity-research.md section 5).
  - The pass weight that lets him meet it without breaking stride is in `docs/design.md` ("Pass weight").
  - A high ball he would be pressed on (ATT-06) he attacks at the highest point he reaches with a header, "just before your opponent" [USS-D p.73], instead of letting it drop to chest height while the defender heads it away. Before, aerial duels ended 2–3 % of 30–45 m lofted passes and opponents headed 12–15 % away unchallenged; now 11–14 % and 6 % (StatsBomb: 14–15 % and ≈ 7 %).
- **Parameters:**
  - `att24_set_margin_s` = 0.3 s (calibrated);
  - `att24_meet_m` = 2.0 m (calibrated; pressed receivers now move a median 1.9 m towards the passer, free ones ≈ 0 m).
  - `reading_receive_delay_max_s` (calibrated against the familiarity bench).
- **Instruction:** per team "come short for the ball" / "hold position" (`TeamOption::ComeShortForTheBall` / `HoldPosition`): `AttackingInstructions::meet_m`.

### ATT-25 · Play away from pressure
Status: v1 · Phase: in possession
- **Source says:**
  - "Players should make good decisions on the ball and avoid forcing passes to team-mates who are under pressure"; "If it is the safer option, players should return the ball to whence it came to ensure that their team retain possession"; "if a central player is not under pressure when they receive the ball, they should look to turn and drive forward" [FIFA-ES, Nielsen].
  - "When receiving under pressure, players should prioritise playing first-time passes" [FIFA-TCP, see ATT-06].
  - Moving with the ball is "more common in attacking areas where there's less at stake from losing possession" [FA-EFL, "How to help your players move with the ball"].
  - The closer to the own goal, the more ball security matters [DFB-FLICK, see ATT-02].
  - Shielding is "holding opponents off the ball" [USS-D, §5 game analysis, physical: strength].
- **Numbers:** the sources give none. In StatsBomb open data (60 matches), receivers under pressure lose the ball before their next pass or shot 21.7 % of the time (dispossessed, miscontrol, possession lost), free receivers 4.9 %, all receivers 5.9 %.
- **Translation (ours):** every on-ball option is valued with the pressure on whoever has the ball after it:
  - the receiver of a pass, at the target when the ball arrives;
  - the carrier at the end of a dribble;
  - the carrier who holds the ball, when his next decision comes.

  "Pressed" is the ATT-06 test (an opponent can reach him within `att06_pressure_time_s` of his arrival), softened by the race sigma. The value of that possession drops by `att25_pressed_loss` × pressed × (its value + the turnover cost there). Before, a dribble under pressure was valued as if the carrier were free at its end. Carriers repeated short dribbles, and spells of ten or more carrying decisions lost the ball 23 % of the time.

  A pressed carrier who holds the ball shields it: he turns his back to the nearest opponent as he perceives him, which puts his body between that opponent and the ball (a challenge from there comes from behind, `tackle_from_behind_penalty`). Free, he faces the opponents' goal. (History, 2026-11-01: until then every holder faced the opponents' goal, so a presser goal-side of him stood in front of him and a team-mate never found him closed to play, PRS-01's double-up trigger.)
- **Parameters:** `att25_receiver_base_loss` = 0 + `att25_receiver_loss` = 0.18 × pressed for the receiver of a pass, `att25_pressed_loss` = 0.3 for the carrier after a dribble or while holding. All calibrated:
  - Receivers: the `pressed_cal` example compares the loss the AI expects with what happens in the engine (now per side with `ATTRS=68 AWAY=47`, with the cause and the time on the ball). At 0.3 it overpriced pressed receivers by about 1.6× (extra loss 20–28 pp expected, 11–20 pp actual), which pushed passes away from near team-mates to free far ones; 0.18 × pressed then fitted the engine, close to the StatsBomb difference of 0.17. After the attribute-slope and direct-play rounds (2026-10-29) the engine's receivers lose the ball before their next pass or shot 3–7 % when free (StatsBomb 4.9 %) and 27–35 % when fully pressed (StatsBomb 21.7 % for its broader "under pressure"), every loss a tackle after a median 1–2 s on the ball, the same for a 47-rated side as for a 68 one, and the same whether the receiver has 1 or 3 free team-mates within 30 m. Pricing that (base 0.05, 0.25 × pressed) was tried and reverted: carriers then passed less and carried more, tackles rose from ≈ 175 to ≈ 215 per team, completion fell 3 points and receivers lost more still (37–40 % pressed, 8–10 % free), see docs/design.md "One-step valuation". The chain beyond the next pass is not mispriced: a team that received under pressure loses the ball within 6 s 55–66 % of the time against 34–38 % after a free reception, and the whole difference is the receiver's own loss.
  - Carriers: a single value of 0.18 for both also made dribbling under pressure cheaper (tackles ≈ 81 → 104 per team), so carrying keeps 0.3, the value at which receivers lose the ball before their next pass or shot 6 % of the time, as real ones do (0.17 gave 8 %, no term 11 %).
- **Instruction:** per team "release the ball early" / "keep the ball under pressure" (`TeamOption::ReleaseTheBallEarly` / `KeepTheBallUnderPressure`): `AttackingInstructions::pressed_loss`; "find the free man" / "play into feet under pressure" (`FindTheFreeMan` / `PlayIntoFeet`): `AttackingInstructions::receiver_loss`.

### ATT-26 · Pin the line or offer short
Status: v1 · Phase: progression / final third
- **Source says:**
  - Passer's state: "If a player has controlled possession and has their head up – they may recognise that they can attempt a run in behind the opposition to receive an incisive pass. If the midfield player has their head down, or is under pressure, the forward player may need to drop-off and provide an option to play to feet"; "The best players know their own cues"; "if you make multiple movements all the way through the game, sometimes showing to feet and sometimes looking for the ball into space, suddenly the defender has a few things to think about" [FA-FWD]. The defending side of the same cue: "Imagine that the opposing player has time on the ball and their striker makes a run in behind. In response, England might drop … But if the opposition player is under pressure and the striker is coming short, they might step up" [FA-EFL, "How to defend like England: space in behind"]. An "open ball" is the "signal to start sprinting forward" [FIFA-HED].
  - Partner: "Bietet sich ein Stürmer zum Zuspiel an, besetzt der jeweils andere die Position in der Tiefe" (if one striker offers for the pass, the other takes the position in depth) [DFB-RP16]. "Der Stürmer zieht durch seinen Laufweg in die Tiefe bzw. seine Stellung im leichten Abseits die Kette weg. Im so vergrößerten Raum zwischen den Reihen kann sich der Zehner anspielbar machen" (the striker's run in depth pulls the back line away; the 10 offers in the space this opens between the lines), and the 10 must not "identisch mit dem Laufverhalten der Spitze immer den Weg in die Tiefe" suchen (always run in depth like the striker) [BDFL-HALE]. "If the number 9 drops deep, the wide players should run into the space left behind" [FIFA-JOHNSON]. The strikers in DFB-SCHOEN go deep while a midfielder offers in the gap (ATT-14).
  - Profile: "Teams used different types of Number 9s, each playing a different specialist role"; teams with pacy wide forwards used the 9 "to drop in to receive between the lines" [FIFA-N9-WC22]. Runners in behind need "the physical ability to move away from the opponent with a short burst and immediately run deep again" [FIFA-GI-BIB]. Receiving between the lines and behind them "work as two sides of the same coin" [FIFA-U17W-CF]. Whether the 9 occupies the centre-backs or comes to feet "really depends on how the Number 9 is expected to play for their team" [FIFA-U20-CF] (an instruction).
- **Numbers:** the sources give none. StatsBomb 360 (open data, open-play passes with the ball 10–50 m from goal; g = a team-mate's distance short of the deepest outfield opponent; `sb_front_pin.py`, `sb_fwd_recv.py` in the scratchpad):
  - Nobody within 2 m of the line at 33 % of passes with the passer free, 38 % pressed. By the line's distance from goal: 25 / 24 % (line 0–12 m), 25 / 26 % (12–18 m), 34 / 40 % (18–25 m), 43 / 49 % (25–35 m), 50 / 58 % (35–60 m).
  - When the front player pins, a second team-mate is ≥ 5 m short of the line in 10–13 % (line 0–12 m) up to 41 % (35–60 m).
  - Centre-forwards' forward receptions in the attacking half: on the line (g < 2) / short (2–8) / dropped (≥ 8 m) 21 / 46 / 33 %; with the line 0–15 m from goal 42 / 42 / 15 %, 30–60 m 9 / 53 / 38 %; passer pressed 20 / 52 / 28 %. Wingers 13 / 37 / 50 %, attacking midfielders 9 / 29 / 62 %.
  - Per centre-forward (≥ 60 receptions) the dropped share spreads p10 15 % / p50 32 % / p90 51 % (Lukaku 9 %, Giroud 15 %, Benzema 51 %, Messi 70 %).
- **Translation (ours; `positioning::possession_movement`, which serves every outfield player in possession, docs/player-brain.md sections 6 and 8):** a front-line player (anchor within `att26_front_m` of the front of the in-possession block, any role) chooses a movement from his role's repertoire (prior > 0, `data/roles`) and hold: **pin** the line (onside on the deepest defender, at the line's speed), **check** from the line 3–8 m towards the ball (straight or angled off, `att26_check_depth_m`), **drop** between the opponents' midfield and back units (ATT-15), or **run in behind** (ATT-18). Each movement is worth what it offers the carrier (`ai::offer`):
  - **Offer:** the ball to the movement's best point, priced as `on_ball::passes` prices a pass (possession value, ATT-17 line breaks, ATT-01 / ATT-03 forward terms, the ATT-25 loss of a pressed receiver, the ATT-02 cost of an interception), with a cheap race along the lane and to the point. The carrier plays only what he sees (vision cone) after turning to it (`execution::setup_time`); a ball in behind only if he is open (facing forward, pressure below `att21_min_pressure`), as well as his passing allows, else with `offer_closed_play`. Player type comes out of the reach and duel models: a runner arrives first more often, a link player (first touch, strength) loses less of a pressed ball to feet.
  - **Marginal value:** what the offer adds to the carrier's expected best option among keeping the ball and his team-mates' offers (where their own movements offer), as the softmax inclusive value at the carrier's decision temperature; an offer within `offer_overlap_m` of a better one adds nothing. "If one striker offers, the other takes the position in depth" [DFB-RP16] follows from it.
  - **Space value:** for a check, drop or run, what the response of the opponent marking him (DEF-04 duty, or the nearest goal-side defender within the zone radius) adds to his team-mates' offers, weighted by his look-ahead (mean of instinct and intelligence).
  - Less **effort** (`effort_per_m` × speed class × distance / work rate); plus the role prior (`role_prior_weight`) and the `forward_runs` / `come_short` instructions (`movement_instruction_weight`, times adherence), all shares of the local value scale; layer-3 noise `movement_noise` × (1 − reliability) and a switch margin for the movement he holds.
  - He re-chooses at a trigger (possession, duty, a pass struck, a new carrier, the carrier turning open or closed), at the periodic check, at the end of a check or drop, and when the line shifts by more than 4 m; with a team-mate's pass in flight that is due within his look-ahead (`lookahead_horizon_s` × look-ahead) he chooses for its receiver (section 8.4).
  - A run waits onside on the line in its lane and sprints when the carrier sets the pass up (ATT-24); the through ball aims at the run's offer (`on_ball::passes`). The offer assumes the run is timed to cross the line at full speed as the pass is struck, the defenders responding to the pass.
- **Parameters:** `att26_front_m` = 5.0 m, `att26_check_depth_m` = 8.0 m; `AiConfig`: `offer_closed_play` = 0.15, `offer_overlap_m` = 6.0 m, `lookahead_horizon_s` = 2.0 s, `effort_per_m` = 0.003, `role_prior_weight` = 0.05, `movement_instruction_weight` = 1.0, `movement_noise` = 0.1, `movement_switch_margin` = 0.05. All calibrated (the sources are qualitative); checked against the pinning share and the per-player spread above with the `forward_moves` bench (docs/design.md "Pin the line or offer short", "Front-line movements, phase D").
- **History:** until phase G (2026-10-31) the team-brain era choice: a softmax over pin / check / drop from the attribute profile, situational cues and `come_short` (`att26_profile_slope` 0.5 per 10 points, `att26_cue_shift` 1.0), scored by a pin bonus (`att26_pin_bonus` 0.6 within `att26_pin_band_m` 2 m of the line) on the positioning candidates. The movement values replaced it (docs/player-brain.md section 13).
- **Instruction:** per player "come short" … "stay on the last line": `PlayerInstructions::come_short`; presets `PlayerPreset::FalseNine` (+1) and `TargetMan` (−1).

## In possession — build-up

### BU-01 · Goalkeeper in possession
Status: v1 · Phase: build-up
- **Source says:** the goalkeeper fulfils "the role of the 11th outfield player" [FA-DNA] and must play with the feet [FFA-NC slide 43]. UEFA: goal kicks that did not leave the box rose from 24 % (2019/20) to 44 % (2022/23); short goal kicks to a teammate in the box 48 % in 2023/24 [UEFA-TO; UEFA-CJ].
- **Numbers:** as quoted.
- **Translation (ours):** the goalkeeper is a regular pass target in build-up (on-ball AI may pass back to him, and he positions as a support option behind the centre-backs, but with the ball within shooting range of his own goal no higher than the GK-01 / GK-03 in-range depth); with the ball he uses the normal on-ball AI. A back-pass to him is always played on the ground, never lofted (a lofted ball towards the own goal risks an own goal; ATT-02 ball security).
- **Parameters:** `bu01_gk_support_depth` = 8.0 m behind the centre-backs (calibrated).

### BU-02 · Full-backs as the free outlet
Status: v1 · Phase: build-up
- **Source says:** "When playing out, it's often the full-backs that can receive the ball from the goalkeeper" (they have no direct opponent) [FFA-NC slide 43].
- **Numbers:** none.
- **Translation (ours):** in build-up the in-possession shape places the full-backs wide at the height of the first pressing line's flank.
- **Parameters:** encoded in the formation's in-possession shape (`data/tactics/*.ron`).

### BU-03 · Centre-backs against a two-man press
Status: v1 · Phase: build-up
- **Source says:** two centre-backs against two pressing forwards "have fewer options"; link with central midfielders or play into the forward [FIFA-TCP]; right-footed #3, left-footed #4 [FFA-NC slide 44].
- **Numbers:** none.
- **Translation (ours):** a pivot (ROLE-07, ROLE-08) can **drop into the back line** between exactly two centre-backs (BU-04); against two pressing forwards the third man in the line is the free one, so the offer values that drop, against one forward it adds little. The feet of the centre-backs are not modelled.
- **Parameters:** none beyond the movement priors.

### BU-04 · Pivot drops
Status: v1 · Phase: build-up
- **Source says:** the #6 finds "the right moment to drop to receive"; "Quick changes in speed … create separation"; "Midfielders can help create space … by dropping short" [FIFA-TCP].
- **Numbers:** none.
- **Translation (ours):** the pivot's **drop into the back line** goes to the midpoint of the two centre-backs; a **show** (ATT-08) is the drop to receive in front of them. Both are chosen by their offers; `forward_runs` −1 favours the drop into the back line.
- **Parameters:** none beyond the movement priors.

### BU-05 · Around, through or over the press
Status: v1 · Phase: build-up
- **Source says:** go around when lanes are blocked, use gaps to reach a player behind the first line, play over midfield into the forward [FIFA-TCP]. More long balls "because teams are pressing high" [UEFA-TO, EURO 2024].
- **Numbers:** none.
- **Translation (ours):** emerges from the on-ball utility: wide, central and long (lofted) pass candidates are all generated and valued by their success probability and line breaks (ATT-17), so whichever route is open is preferred.
- **Parameters:** none.

### BU-06 · Box midfield
Status: v1 · Phase: build-up
- **Source says:** "teams creating a four in the centre of the pitch … often in a 3-2-2-3" [UEFA-TO; UEFA-CJ].
- **Numbers:** none.
- **Translation (ours):** a full-back or wing-back can **invert** into midfield, beside the pivot (at the pivot's depth, 10 m from the centre) or 8 m ahead of it, which with the pivot pair makes the central four. Chosen by its offer; the `invert` instruction (layer 2, "move inside") makes it his movement.
- **Parameters:** none beyond the movement priors.

## In possession — final third

### FT-01 · Width to open the centre
Status: v1 · Phase: final third
- **Source says:** use the width "to stretch the opposition and create gaps in central areas"; circulate through central triangles to "lure" the opponent, then go wide [FIFA-ES Zamrat; FIFA-TSG].
- **Numbers:** none.
- **Translation (ours):** no rule of its own. The width comes from ATT-11's final-third width (46 m against 43–45 m real), and luring and then going wide is ATT-21's switch away from a crowded ball side.
- **Parameters:** none.

### FT-02 · Cut-backs and low crosses
Status: v1 · Phase: final third
- **Source says:** cut-back: "a ball that is played diagonally backwards from close to the by-line"; WC2022 goals from crosses rose 25 → 45, cut-back goals 2 → 7 [FIFA-TSG]; UCL 2023/24: 30 % of group-stage goals from low crosses or cut-backs [UEFA-TO]; in the 2023/24 knockout-stage crossing analysis these deliveries led to a shot 38 % of the time [UEFA-CJ].
- **Numbers:** open-play crosses 7.7 per team-match (× 1.48 = 11.4), 30 % completed, 16 % followed by a shot; cut-backs 0.62 per team-match, 62 % completed; real high-cross hang times 0.87 / 1.23 / 1.62 s at 0–15 / 15–25 / 25–35 m, median horizontal speed 17 m/s (StatsBomb 360 open data, 166 matches).
- **Translation (ours):** a cross is a pass from the wide final third into the box (`is_cross`): to a team-mate's feet or to the spot a box run offers (FT-03), lofted at `cross_launch_angle` or driven along the ground. A cut-back is a ground ball from near the by-line back into the box. A cross flies over the lane (no lane interception and no ATT-10 screening). At the target, opponents who reach it in time contest it in the air (`aerial_win_prob`), and the keeper races only its flight time at 17 m/s (GK-04). Its availability is 0.5 + 0.5 × crossing.
- **Parameters:** `cross_launch_angle` = 0.33 rad (calibrated against the real hang times).
- **Bench (`examples/wide.rs`):** 11.5 crosses per team-match, 30.5 % completed, 13.2 % followed by a shot. By band (StatsBomb x 80–96 / 96–108 / 108–120), 7.6 / 2.9 / 1.0 per team-match against 2.2 / 3.8 / 5.4 real, so cut-backs are 0.25 against 0.92. Crosses come too early because the alternatives in deep wide positions are undervalued (the possession value's `value_wide`, open). Outcomes per band match the real ones (completed 30 / 32 / 28 %, shot 12 / 16 / 18 %).

### FT-03 · Box occupation
Status: v1 · Phase: final third
- **Source says:** on average 3.3 players in the box when the ball arrives from wide [FIFA-TSG]; one forward "attacks the front post", the opposite winger the "back post", a midfielder "the edge of the penalty area for any second balls" [FIFA-ES]; (a further claim about a centre-forward "ready for a cut-back" was not found in [FFA-NC] and was removed).
- **Numbers:** 3.3 attackers in the box when the ball arrives from wide [FIFA-TSG]; 2.35 team-mates in the box at the release (StatsBomb 360 open data).
- **Translation (ours):** `Movement::BoxRun` is in the in-possession repertoire. It is offered while the carrier is in the attacking third outside the width of the box, and its offer points are the spots `ft03_spots` (near post, far post, penalty spot, edge of the box). They are valued like every offer, by marginal value, so runners take different spots. The runner holds onside until a team-mate's cross is in the air, then runs onto his spot, and keeps it until the cross lands (`offer::Passer::cross_from`; history, 2026-11-01: once the cross was struck, the carrier the runners offered to became its receiver inside the box, so the box run lapsed in mid-flight and they re-chose "support" or "rest defence", seen at corners). The carrier sees a box runner's spot (`reading::seen`) and plays a lofted cross or a ground ball to it. Instruction `forward_runs`.
- **Parameters:** `ft03_spots` = (7, 3), (7, −4), (11, 0), (17, 4) m (depth from the goal line, lateral towards the ball side; calibrated: the post spots at the six-yard line, the edge for second balls).
- **Bench:** team-mates in the box 0.59 at the release (2.35 real) and 1.0 at the first contact (3.3 at arrival [FIFA-TSG]). Runners wait onside behind a line that is still outside the box (CRS-01 lag), and crosses from deep leave no time to fill it (open).

### FT-04 · Finishing profile
Status: metric · Phase: final third
- **Source says:** "Some 80% of goals are one-touch finishes" [FIFA-TSG, Game Insights "Using wide areas"]; UCL 2023/24 group stage: 63 % one-touch, 68 % under direct pressure; 53 % of goals from moves of 1–5 passes [UEFA-TO].
- **Translation (ours):** calibration target for the shot / xG models, reported by `match_lab`.

## Attacking transition

### TR-01 · Speed and forward play on the counter
Status: v1 · Phase: counter-attack
- **Source says:** "Take a minimal number of touches and passes to play forward"; pass "into space rather than into feet" [FIFA-ES]. UEFA keys: "First pass out of the counterpressure", "Speed through forward passes", "Intense forward runs"; counter examples: Leverkusen 9 s / 70 m, Monaco 10 s with four forward passes, Bodø/Glimt 11 s / 74 m with "no player touching the ball more than twice" [UEFA-PI 2026]. "6 seconds" transition window, "Finishing in 3-4 Passes" [FA-HUNTS].
- **Numbers:** as quoted.
- **Translation (ours):** in counter-attack mode, forward passes into space and forward dribbles get a bonus, backward passes a penalty, and holding the ball is discouraged.
- **Parameters:** `tr01_forward_bonus` = 0.25; `tr01_backward_penalty` = 0.25 (calibrated, fractions of the local value scale).

### TR-02 · First-pass direction after regain
Status: v1 · Phase: attacking transition
- **Source says:** won centrally → first pass wide into the space left by the opposing full-back; won wide → centre-forward drops to support, a central player attacks the box; "Play the ball away from pressure and into space" [FIFA-ES Boumehdi, Urias].
- **Numbers:** none.
- **Translation (ours):** no rule of its own. In a counter (TR-01) the first pass is priced like every pass: a pressed receiver keeps less of it (ATT-25), and the space is the possession value where he receives, so the pass goes away from the pressure into space. The direction by where the ball was won (central → wide, wide → the centre-forward drops) emerges from where that space is.
- **Parameters:** none (TR-01's `tr01_forward_bonus`, `tr01_backward_penalty`).

### TR-03 · Running lanes on the counter
Status: v1 · Phase: counter-attack
- **Source says:** runners "should coordinate their runs to avoid attacking the same area"; push up as a unit [FIFA-ES Urias, Boumehdi].
- **Numbers:** none.
- **Translation (ours):** no rule of its own. A run into the area a team-mate's run already attacks adds little to the carrier's best option (marginal value), so runners on the counter take different lanes. Test 10 (`tests/brains.rs`) checks, with perfect readers, that two runs in behind end within `offer_overlap_m` of each other in under 1 % of counter-attack ticks.
- **Parameters:** none.

### TR-04 · Counter or keep the ball
Status: v1 · Phase: attacking transition
- **Source says:** players must "recognize … when to possess the ball or to penetrate quickly" [USS-D p.54]; ball retention to manage momentum [FA-DNA].
- **Numbers:** no criteria given by the sources.
- **Translation (ours):** at regain the team counter-attacks if its players ahead of the ball are at least the opponents between the ball and their goal minus `tr04_numbers_margin`, and the regain is not deep in the own third under pressure; otherwise it secures possession (build-up / progression).
- **Parameters:** `tr04_numbers_margin` = 1 (calibrated).

### TR-05 · Rest defence
Status: v1 · Phase: in possession
- **Source says:** rest defence is "the defensive structure a team adopts when in control of the ball"; Spain used a box of four [FIFA-TSG]. UEFA: "2+3 or 3+2" against five in the opponent's front line [UEFA-TO]. FFA: one of #6/#8 joins the attack "while the other one stays behind the ball" [FFA-NC slide 44]. DFB: every player without the ball must be available or able to attack his opponent if the ball is lost [DFB-FLICK; DFB-SCHOEN p.21].
- **Numbers:** 4–5 players behind the ball.
- **Translation (ours):** while in possession, the `tr05_min_players` deepest outfield players (5 when the opponents leave five or more players high) stay at least `tr05_gap` behind the ball. Rest defenders choose from hold, show, invert, drop into the back line and step into midfield, every point no further forward than `tr05_gap` behind the ball or, while the ball is behind that, than their own spot in the shape. The claim order is situational: a wide player on the ball's side claims later (`tr05_ball_side_release`, he is the one who can join the attack, "2+3 or 3+2" [UEFA-TO]), and the player instructions move the claim (`tr05_instruction_shift` × (`forward_runs` + `overlap`) × adherence: "stay back" claims earlier, "overlap" later). A player knows a team-mate's instructions as far as the team has practised the concept, and holds the rest-defence and support claims he has read.
- **Parameters:** `tr05_min_players` = 4 (sourced); `tr05_max_players` = 5 (sourced); `tr05_gap` = 3.0 m (calibrated); `tr05_ball_side_release` = 0.3 formation depth (calibrated); `tr05_instruction_shift` = 0.5 (calibrated: "stay back" puts a full-back before the far full-back).

## Laws and set pieces

The Laws of the Game are rules, not tactics, but the engine needs their translation in the same traceable form. The laws themselves are quoted from the IFAB's 2026/27 text. The set-piece positions (SP-01…05) are layer 1 like any other tactical rule, and are resolved per team (`SetPieceTactics` in `crates/engine/src/team/tactics.rs`, instructions `SetPieceInstructions`). Engine counts are compared with real per-match counts × the lab's in-play ratio (≈ 1.6, printed by `match_lab`).

### LAW-01 · Offside
Status: v1 · Phase: all
- **Source says:** "A player is in an offside position if" both "any part of the head, body or feet is in the opponents' half (excluding the halfway line)" and "any part of the head, body or feet is nearer to the opponents' goal line than both the ball and the second-last opponent" (Law 11.1). "A player in an offside position at the moment the ball is played or touched" by a team-mate "is only penalised on becoming involved in active play" (Law 11.2). "There is no offside offence if a player receives the ball directly from:" a goal kick, a throw-in, a corner kick (Law 11.3). "If an offside offence occurs, the referee awards an indirect free kick where the offence occurred" (Law 11.4) [IFAB-L11]. Runners time their runs on "the last defender or the assistant referee on the touchline"; "Using curved runs can also help attackers time their movement while remaining onside" [FIFA-OFF02].
- **Numbers:** offsides per match (both teams) 4.1 (EPL 2018/19), 3.1–4.5 across the big five leagues [ACAD-OFF01]; StatsBomb open data 1.49 per team-match.
- **Translation (ours):** when a player plays the ball (a pass, shot, dribble touch, header or a controlling touch, `Simulation::played`), his team-mates in an offside position at that moment are marked, none from a goal kick, throw-in or corner. A deliberate play by an opponent clears the marks; a save or a blocked kick does not. A marked player who plays the ball or challenges for it is offside: the pass is lost and the opponents get an indirect free kick where he was. The passer judges offside himself, against the line as he perceives it (ATT-05 scanning) with a judgement error of σ = `offside_judge_m` × (1.25 − 0.5 × intelligence) m (`on_ball::judged_offside`); runs hold onside as before (ATT-18).
- **Parameters:** `offside_judge_m` = 2.8 m (calibrated against offsides per team-match).
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) offsides 1.25 against 2.41: too few (open).

### LAW-02 · Fouls, misconduct and advantage
Status: v1 · Phase: all
- **Source says:** "A direct free kick is awarded if a player commits any of the following offences against an opponent in a manner considered by the referee to be careless, reckless or using excessive force"; "Careless is when a player shows a lack of attention or consideration when making a challenge or acts without precaution"; "Reckless is when a player acts with disregard to the danger to, or consequences for, an opponent and must be cautioned"; "Using excessive force is when a player exceeds the necessary use of force and/or endangers the safety of an opponent" (Law 12.1). Denying an obvious goal-scoring opportunity weighs "distance between the offence and the goal", "general direction of the play", "likelihood of keeping or gaining control of the ball", "location and number of defenders and attackers"; in the penalty area "the offender is cautioned if the offence was an attempt to play the ball or a challenge for the ball"; a caution for an offence "which interferes with or stops a promising attack"; "receiving a second caution in the same match" is a sending-off (Law 12.3–12.4) [IFAB-L12]. The referee "allows play to continue when an offence occurs" and "the non-offending opposing team will benefit from the advantage", and "penalises the offence/orders a retake if the anticipated advantage does not ensue at that time or within a few seconds" (Law 5.3); the caution is given "when play next stops" [IFAB-L5].
- **Numbers:** StatsBomb open data (40 matches) per team-match: 12.1 fouls (fouler's own / middle / attacking third 17 / 53 / 30 %), 1.64 advantages, 2.12 yellow cards, 0.03 red cards and 0.01 second yellows, 0.11 penalties.
- **Translation (ours):** a challenge (IND-03 tackle, or an aerial duel) is a foul with a probability by its outcome: `foul_beaten` when the tackler missed the ball and was beaten, `foul_deflected` when he knocked it loose, `foul_won` when he won it, `foul_aerial` for the loser of an aerial duel; × `foul_from_behind` from behind and × (1 ± `foul_tackling`, `foul_aggression`) from the best to the worst tackler and from the calmest to the most aggressive player. A foul is reckless (a caution) with `card_reckless`, × `card_from_behind` from behind, and uses excessive force (a sending-off) with `red_excessive`. SPA and DOGSO come from the circumstances the Law lists: the distance to goal (`spa_distance`, `dogso_distance`), the fouled player heading for goal, and the outfield defenders goal-side of him (`spa_defenders`); every engine foul is a challenge for the ball, so a DOGSO in the penalty area is a caution. Advantage: a fouled carrier who beat the challenge stays on his feet with `stay_min` + `stay_range` × strength; in his attacking half (not in the penalty area) the referee then lets play go on, and brings it back for the foul if the fouled team loses the ball within `advantage_s`. A deferred caution is shown at the next stoppage (an SPA then earns none). Layer 3: a cautioned player commits to a tackle only at `caution_margin` better odds, fouls and is reckless `caution_care` × as often, and commits no tactical fouls (TRN-04); in his own penalty area every defender fouls `foul_box` × as often (he stays on his feet). A foul in the offender's penalty area is a penalty kick (SP-04), elsewhere a direct free kick (SP-03); a sent-off player leaves, and his team keeps its shape with ten.
- **Parameters:** `foul_beaten` = 0.40, `foul_deflected` = 0.155, `foul_won` = 0.04, `foul_aerial` = 0.05, `foul_from_behind` = 2.0, `foul_tackling` = 0.5, `foul_aggression` = 0.5, `card_reckless` = 0.15, `card_from_behind` = 2.0, `red_excessive` = 0.002, `spa_distance` = 35 m, `spa_defenders` = 1, `dogso_distance` = 25 m, `stay_min` = 0.2, `stay_range` = 0.4, `advantage_s` = 3.0 s (sourced: "within a few seconds"), `fouled_down_s` = 1.5 s, `caution_margin` = 0.1, `caution_care` = 0.15, `foul_box` = 0.13 (all calibrated against the StatsBomb counts unless marked; `RulesConfig`).
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) fouls 19.4 against 19.6, by the fouler's own / middle / attacking third 26 / 65 / 9 % against 17 / 53 / 30 %: too few high up the pitch (open); advantages 4.3 against 2.7; yellow cards 3.35 against 3.43; red cards 0.14 against 0.065 (11 in 80 team-matches, 8 of them direct, 3 second yellows); penalties 0.16 against 0.18.

### LAW-03 · Added time
Status: v1 · Phase: all
- **Source says:** "Allowance is made by the referee in each half for all playing time lost in that half through: substitutions assessment and/or removal of injured players wasting time disciplinary sanctions medical stoppages … goal celebrations any other cause, including any significant delay to a restart"; "The fourth official indicates the minimum additional time decided by the referee at the end of the final minute of each half. The additional time may be increased by the referee but not reduced" (Law 7.3) [IFAB-L7]. Goal celebrations became a separate bullet in 2023/24 "to emphasise that they often result in a significant amount of time being lost" [IFAB-CH23]. Delays: goal 60.3 ± 18.9 s, substitution 45.2 ± 16.1 s (WC 2014 knockout matches) [ACAD-ST01].
- **Numbers:** Premier League average stoppage time per match (both halves) 6:13–6:40 (2014/15–2018/19), 8:27 (2022/23), 11:39 (2023/24) [PL-ST24]; StatsBomb open data, last event of each half +2.0 / +4.9 min (older seasons).
- **Translation (ours):** each half starts with `added_base_s` for the losses the engine does not play out (injuries, time-wasting, "any other cause"); every goal adds `added_goal_s`, every substitution `added_sub_s`, every caution or sending-off `added_card_s`, every penalty kick `added_penalty_s`. At the end of regular time the minimum is shown in whole minutes (`EventKind::AddedTime`); what is lost after that extends it. The engine plays the added time (it does not stop the clock during its own restarts, which take their real time to organise).
- **Parameters:** `added_base_s` = 60, `added_goal_s` = 60 (sourced [ACAD-ST01]), `added_sub_s` = 30, `added_card_s` = 20, `added_penalty_s` = 60 (calibrated against PL 2023/24).
- **Bench:** (round h, 40 matches) shown 3.9 / 7.8 minutes, played 12.4 minutes per match, against 11:39 in the Premier League 2023/24 [PL-ST24].

### LAW-04 · Substitutions
Status: v1 · Phase: all
- **Source says:** the number of substitutes, "up to a maximum of five", is set by the competition; each team "has a maximum of three substitution opportunities" and "may additionally make substitutions at half-time" (Law 3) [IFAB-L3]. Under the five-substitute rule teams use 4.25–4.32 [ACAD-SUB02] or 4.26 ± 1.07 [ACAD-SUB03] substitutes; first substitution ≈ minute 56–57, last ≈ 82–83 [ACAD-SUB02]; losing teams substitute earlier [ACAD-SUB03]. Reasons include "the threat of the substituted player receiving a yellow-red card" [ACAD-SUB02].
- **Numbers:** StatsBomb open data: 4.39 substitutions per team-match; minute p10 / p25 / median / p75 / p90 = 51 / 61 / 72 / 80 / 87; 8 % at half-time.
- **Translation (ours):** until a manager makes them, each team's default policy substitutes at stoppages. From `sub_from_min`, once an outfield player's long-term energy reserve (M6 fatigue) has fallen below `sub_reserve` + `sub_reserve_per_min` × (minutes past `sub_from_min`), a window opens and the players within `sub_bundle` of that threshold are replaced, the most tired first (a cautioned player counts `sub_caution` more tired), at most `sub_per_window` per window, within `max_subs` substitutes and `max_sub_windows` windows (half-time uses none). The substitute is the bench player who suits the role best: role familiarity × 1 for the same role, 0.85 for the same unit, 0.6 otherwise (`PlayerData::suitability`).
- **Parameters:** `max_subs` = 5, `max_sub_windows` = 3 (sourced, Law 3); `sub_from_min` = 45, `sub_reserve` = 0.58, `sub_reserve_per_min` = 0.0022, `sub_bundle` = 0.04, `sub_per_window` = 2, `sub_caution` = 0.1 (calibrated against the StatsBomb minutes).
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) 4.56 substitutions, minute p10 / median / p90 57 / 69 / 81, against 4.39 and 51 / 72 / 87 (unscaled).

### SP-01 · Defending corners
Status: v1 · Phase: set pieces
- **Source says:** at WC 2022 "only five defended with a player positioned on at least one post, and only six teams left a player higher up the pitch … 26 teams brought all ten outfield players back"; "the most popular defensive setup … was a hybrid approach in which teams used a combination of zonal marking, player-to-player markers and 'edge' players"; a setup is zonal "when a team assigns zonal … defensive responsibilities to four or more players"; 15 of 32 teams used two or three zonal players [FIFA-SP01]. At the 2025 Club World Cup zonal marking rose to 60 % of corners; "When teams had no players designated to protect the edge of the penalty area, 57% of corners entered a second phase, compared to 40% when the defending team had one" [FIFA-SP02]. "The starting position for a goalkeeper remains consistent … regardless of the defensive setup"; "For inswingers, the goalkeeper starts closer to their goal line" [FIFA-GK01]. WC 2018: mixed marking 88.8 %; "teams tend not to position players on the goalposts (72.2%)" [ACAD-C01].
- **Numbers:** as quoted; StatsBomb open data: 4.97 corners per team-match, a shot within 10 s after 1.61, a goal after 0.11.
- **Translation (ours):** while the corner is prepared (between `sp01_setup_min_s` and `sp01_setup_max_s`, until every player is within 2.5 m of his spot) the defenders take set positions (`ai::set_piece`): the keeper `sp01_gk_m` off his line; `counter` outfield players (the fastest) stay up as outlets; the `zonal` best in the air hold zones in and around the goal area (no one on the posts); one `edge` player at the edge of the box; the rest mark the attackers in the box player to player, a metre goal-side, the best headers on the best headers; anyone left over holds a zone. Once the kick is taken, open play takes over (CRS-01, GK-04, aerial duels).
- **Parameters:** `sp01_zonal` = 3 (sourced: two or three most common), `sp01_edge` = true (sourced), `sp01_counter` = 0 (sourced: 26 of 32 teams), `sp01_gk_m` = 1.0 m, `sp01_setup_min_s` = 3 s, `sp01_setup_max_s` = 15 s (calibrated).
- **Instruction:** per team "zonal marking at corners" / "man-to-man at corners" (`TeamOption::ZonalAtCorners` / `ManToManAtCorners`): `SetPieceInstructions::zonal`; "leave players up at corners" / "everyone back at corners" (`LeavePlayersUp` / `EveryoneBack`): `counter`; `edge`.
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) corners 1.95 against 8.03; a shot / goal within 10 s after 0.55 / 0.075 against 2.60 / 0.178, about the real rate per corner. Two thirds of the engine's corners follow a save or a block: its defenders seldom clear the ball (4.7 clearances against ≈ 29, open).

### SP-02 · Attacking corners and free kicks
Status: v1 · Phase: set pieces
- **Source says:** "Commit 5-6 players to attack in tiered formation. Engage player-for-player markers with multiple attackers. Overload front post or win flick-on at front post. Use third man for short corners Target front post or centre of goal" [FIFA-SP04; similarly FIFA-SP05]. "as many as six players attacking corners" [UEFA-UCL24]. Deliveries: first post 42.3 %, centre 23.8 %, second post 20.5 %; short 20.5 % [ACAD-C01]. Short corners 17.7–18.8 % in the Premier League [PL-TT24].
- **Numbers:** as quoted.
- **Translation (ours):** the corner's delivery is the team's routine, drawn per kick (as SP-04's kick side): short with `sp02_short_share` when there is a short option (the best ground pass to a team-mate outside the box), otherwise a lofted ball to the first post, the centre or the second post by the `sp02_delivery` weights, to the attacker nearest that zone (`set_piece::corner_delivery`). History: as the taker's on-ball choice, 86 % of the corners went along the ground to the front post and 13 % in the air, none short. The ground ball to a marked attacker was priced as possession 5 m from goal (≈ 0.27), the header from the same ball ≈ 0.155, so the softmax took the ground ball nine times in ten.
  - Positions: at a corner the `attackers` best in the air (excluding the taker, the best crosser) take tiered spots: the centre of the goal area, the front post, the far post, a second wave at the front post, the penalty spot, and one screening the keeper. The best long-range shooter waits at the edge of the box for the second ball; with `short`, the best short passer offers a short option near the corner; the rest stay back as rest defence (TR-05). At a free kick in the attacking third the attackers stand level with the defensive line, onside, spread across the goal. The delivery from a free kick is the taker's on-ball choice like any pass (FT-02 crosses).
- **Parameters:** `sp02_attackers` = 5 (sourced: 5–6), `sp02_short` = true (sourced), `sp02_short_share` = 0.19 (sourced: 17.7–18.8 % [PL-TT24], 20.5 % [ACAD-C01]), `sp02_delivery` = [0.423, 0.238, 0.205] (sourced [ACAD-C01]; normalised when drawn).
- **Instruction:** per team "commit players to the box" / "hold back at set pieces" (`TeamOption::CommitToTheBox` / `HoldBackAtSetPieces`): `SetPieceInstructions::attackers`; `short`; the routine: `short_share`, `delivery`.
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) short corners 0.00 against 0.97: the taker never chose the short option. With the routine (2026-11-01, lab 40): 14 % of the corners short (0.19 of 1.39 per team-match), a shot within 10 s after 0.44 corners (was 0.28; real 0.29–0.36).

### SP-03 · Free kicks near goal: wall, line and keeper
Status: v1 · Phase: set pieces
- **Source says:** "Where three or more defending team players form a 'wall', all attacking team players must remain at least 1 m (1 yd) from the 'wall'" (Law 13) [IFAB-L13]. "Centrality and proximity to the goal increased the variables PLAYERS IN WALL …"; goals rose "from 5.9% (central far) to 10.9% (central near)" [ACAD-FK01]. "goalkeepers tend to dictate the number of players necessary based upon the distance of the shot, meaning the further away, the smaller the wall. Also, you want the last man … to station himself one yard beyond the near post" [LOW-FK03]. "As the wall protects one side of the goal, the goalkeeper often shifts to cover the perceived 'vacated' area behind it" [FIFA-SP06]. Against a long, wide free kick the keeper "must defend the space … 3.71m from his goal line and just over 12m from his defensive line" [FIFA-GK02]. "The average Expected Goal ('xG') associated with a direct free-kick is 0.06" [DATA-FK02]; La Liga direct free-kick conversion 2.4–2.8 % [OPTA-FK01].
- **Numbers:** StatsBomb open data: 0.41 direct free-kick shots per team-match (0.12 on target, mean xG 0.042).
- **Translation (ours):** a direct free kick within `sp03_range_m` of the goal gets a wall of 1 + (`wall_max` − 1) × closeness × centrality players (closeness 1 at 18 m, 0 at the range; centrality the goal angle against a central kick from the same distance), the weakest in the air, the end man a yard outside the near post on the ball's line to it, shoulder to shoulder towards the middle, 9.15 m from the ball; the keeper shifts `sp03_gk_shift_m` to the far side. A free kick to be crossed gets a line at `line_m` from the goal line (a one-man wall only for a wide kick near the box), the keeper `sp03_gk_m` off his line, markers goal-side but no deeper than the line. The taker (the best crosser or long-range shooter) chooses between passes and, for a direct kick, a single shot variant "over the wall" (`sp03_speed` × his kick speed, `sp03_height` m at the goal line); its xG ignores the wall's blocking and is × `sp03_xg`.
- **Parameters:** `sp03_range_m` = 32 m, `sp03_wall_max` = 5, `sp03_gk_shift_m` = 1.0 m, `sp03_line_m` = 16 m, `sp03_gk_m` = 3.7 m (sourced [FIFA-GK02]), `sp03_speed` = 0.68, `sp03_height` = 1.9 m, `sp03_xg` = 0.67 (calibrated against the StatsBomb direct shots and xG).
- **Instruction:** `SetPieceInstructions::wall_max`, `line_m`.
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) 17.0 free kicks awarded; direct shots 0.80 against 0.66, on target 0.21 against 0.19, xG 0.068 each against 0.04–0.06.

### SP-04 · Penalty kicks
Status: v1 · Phase: set pieces
- **Source says:** the goalkeeper "must remain on the goal line, facing the kicker, between the goalposts, until the ball is kicked"; the other players "at least 9.15 m (10 yds) from the penalty mark", "behind the penalty mark", "outside the penalty area" (Law 14.1) [IFAB-L14]. "a right-footed kicker … will find it easier to kick to his left (his 'natural side')"; kicks natural side 44.9 %, middle 17.2 %, other side 37.9 %; keepers dive to the kicker's natural side 56.6 %, stay 2.4 %; scored natural side 63.6 % / 94.4 % when the keeper guesses right / wrong, other side 43.7 % / 89.3 % [ACAD-PK02]. Keepers stay in the centre on 6.3 % of kicks; "about 80% of the penalty kicks resulted in a goal" [ACAD-PK01]. "The expected goals (xG) value of every penalty is 0.78 because historically about 78% of them are scored on average" [OPTA-PK01].
- **Numbers:** as quoted; StatsBomb open data 7 of 9 converted.
- **Translation (ours):** the best finisher (finishing + composure) takes it; the keeper stands on his line, everyone else just outside the area (`sp04_edge_m`). The kicker goes natural side with `sp04_natural`, middle with `sp04_middle`, otherwise the other side (a hashed choice per kicker), aiming `sp04_aim_m` from the centre at `sp04_height` m with `sp04_speed` × his kick speed and his normal execution error. As the kick is struck the keeper stays with `sp04_keeper_stays`, else dives to the kicker's natural side with `sp04_keeper_natural`. He has guessed right if he dived to the side the ball goes, or stayed and it goes within `sp04_centre_m` of the centre; then he moves at once and saves `sp04_save_right` × (0.5 + reflexes). One who guessed wrong is diving the other way. The kick's xG is `sp04_xg`.
- **Parameters:** `sp04_natural` = 0.449, `sp04_middle` = 0.172, `sp04_keeper_stays` = 0.063, `sp04_keeper_natural` = 0.58, `sp04_xg` = 0.78 (sourced); `sp04_centre_m` = 1.5 m, `sp04_edge_m` = 1.0 m, `sp04_aim_m` = 2.2 m, `sp04_height` = 0.5 m, `sp04_speed` = 0.8, `sp04_save_right` = 0.3 (calibrated: 36–56 % of kicks not scored when the keeper guesses right, misses included [ACAD-PK02]).
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) 0.16 penalties, 77 % scored, against 0.18 and 78–80 %.

### SP-05 · Goal kicks
Status: v1 · Phase: set pieces
- **Source says:** "The ball is in play when it is kicked and clearly moves"; "Opponents must be outside the penalty area until the ball is in play" (Law 16) [IFAB-L16]. Since 2019 "the ball is in play once it is kicked, i.e. it does not have to leave the penalty area" [IFAB-1920]. "Usually the goalkeeper plays the ball out to one side and then, more often than not, receives the ball back" [FIFA-GK03]. "the trigger for the USA's centre-forwards to press was the taking of the goal kick itself, whilst cutting off any return pass to the goalkeeper"; "Having two or more pressing players within 20 metres of the opposition's goal kick has become increasingly popular" [FIFA-PR01; FIFA-PR02].
- **Numbers:** half of the Premier League teams play short goal kicks most of the time (2024/25) [PL-TT24]; StatsBomb open data 7.59 goal kicks per team-match, 3.85 short (< 32 yd).
- **Translation (ours):** no positions of their own. The opponents keep outside the penalty area (`Simulation::off_ball`), the keeper takes it, and the build-up (BU-01…05) decides short or long like any keeper's pass. The press on it is the open-play press (PRS-01, goalkeeper in possession); a goal-kick routine of its own (forwards pressing on the kick, cutting the return pass [FIFA-PR01]) is not modelled.
- **Parameters:** none.
- **Bench:** (round h of M7, `match_lab` 40 matches, per team-match; real StatsBomb counts × 1.62 for the time in play) goal kicks 6.9 (5.8 short) against 12.3 (6.2): too few, like the corners (fewer balls go out), and too many short (open).


### SP-06 · Throw-ins
Status: v1 · Phase: set pieces
- **Source says:** at the moment of delivery the thrower must "stand facing the field of play", "have part of each foot on the touchline or on the ground outside the touchline" and "throw the ball with both hands from behind and over the head from the point where it left the field of play"; "All opponents must stand at least 2 m (2 yds) from the point on the touchline where the throw-in is to be taken"; "A goal cannot be scored directly from a throw-in" (Law 15) [IFAB-L15]. "male players have recorded release speeds of 12–19 m/s and release angles of 22–40º"; one player's throws at his preferred angle: "17.0 ± 0.8 m, 13.4 ± 0.3 m/s, 2.23 ± 0.02 m and 32.1 ± 1.6°" (distance, release speed, release height, release angle) [LIN-TI01].
- **Numbers:** as quoted; StatsBomb open data 15.9 throw-ins per team-match.
- **Translation (ours):** the thrower's options are the passes of open play up to 25 m (offside-exempt, Law 11.3), every one thrown: released at `sp06_height` and `sp06_angle` without spin, its first bounce at the target, at up to `sp06_speed_min` + (`sp06_speed_max` − `sp06_speed_min`) × strength. History: until 2026-11-01 throw-ins were kicked from the ground, 87 % as ground passes.
- **Parameters:** `sp06_height` = 2.2 m, `sp06_angle` = 0.56 rad (32°), `sp06_speed_min` = 12 m/s, `sp06_speed_max` = 19 m/s (sourced [LIN-TI01]; the mapping onto strength is ours).
- **Bench:** throws leave from 2.2 m and peak at 3–5 m (`film`, 23 throw-ins of one match). The throw's accuracy still comes from the passing skills (open).
---

## Calibration targets (published match data)

`match_lab` reports these for emergent behaviour; `calibrated` parameters are tuned against them. Since M8 its
scorecard checks each against its range: shapes in a phase, shares and rates per event are targets; per-match volumes
(real counts × the in-play scale) depend on the teams' styles and are listed as references only. Where a row gives
only a mean, the scorecard's range is ± 10 % of it.

| Metric | Target | Source |
|---|---|---|
| Mid-block team length | ≈ 27.2 m (tournament average; teams 22.5–29.1 m) | [FIFA-WC22-MB] |
| Mid-block team width | ≈ 40.1 m (tournament average; teams 38.4–40.75 m) | [FIFA-WC22-MB] |
| Mid-block line height (from own goal) | ≈ 37.2–37.9 m | [SPORMETRE-WC22] |
| High block / press: length × width, line | ≈ 36.5–36.8 × 40.2–40.9 m, line 47.5–48.0 m | [SPORMETRE-WC22] |
| Low block: length × width, line | ≈ 24.7–25.5 × 35.5–36.0 m, line 17.6–18.1 m | [SPORMETRE-WC22] |
| In possession, build-up (own third) | ≈ 39.2–39.5 × 53.5–54.5 m, line 19.5–19.7 m | [SPORMETRE-WC22] |
| In possession, build-up (middle third) | ≈ 32.8–33.2 × 55.0–55.5 m, line 40.0–40.6 m | [SPORMETRE-WC22] |
| In possession, final third | ≈ 35.8–36.6 × 43.3–45.2 m, line 52.6–53.7 m | [SPORMETRE-WC22] |
| Goalkeeper distance to goal line | ≈ 13.1 m (average) | [FIFA-GKH] |
| Goalkeeper distance to defensive line | ≈ 20.4 m (average) | [FIFA-GKH] |
| Pressers per press | mostly 2–3 (single-team evidence: Ajax 58 % 2–3, ~25 % 4+) | [UEFA-PI] |
| Counter-press window | ≈ 5 s | [BDFL-KRUE; UEFA-PI] |
| Players in the box on wide entries | ≈ 3.3 | [FIFA-TSG] (M4) |
| One-touch goals | 63–80 % | [FIFA-TSG; UEFA-TO] |
| Goals from moves of 1–5 passes | ≈ 53 % | [UEFA-TO] |
| Counter-attack duration / distance | ≈ 9–11 s / ≈ 70 m | [UEFA-PI] |
| Open-play passes of 30–45 m | ≈ 110 per match (7.8 % of passes): forward 77 (55 % completed, 66 % high), lateral 11, backward 22 | StatsBomb open data, 40 matches ×1.48 (`passes` example) |
| Receipt → next pass or shot of the same player | median 1.22 s; first time (≤ 0.1 s) 18 %, 0.1–0.5 s 7 %, > 1 s 59 % (shots alone: 47 % first time) | StatsBomb open data, 40 matches (`settle` example) |
| Nearest opponent to the passer at a pass | median 2.9 m (own / middle / final third 3.4 / 2.9 / 2.3 m) | StatsBomb 360, 60 matches (`support` example) |
| Switches of play (≥ 25 m across to the other half of the width) | ≈ 98 per match, 79 % completed; ≥ 34 m ≈ 34 | StatsBomb open data, 40 matches ×1.48 (ATT-21) |
| Open-play high passes of 30–45 m | 57.8 per match, 49 % completed, 6 % out of play; an aerial duel at the end 14–15 %, won by the passing team 41 % | StatsBomb open data, 166 matches ×1.48 (`high_balls` example) |
| High passes of 30–45 m by the race at release (nearest opponent minus nearest team-mate to the end point) | completed: < −5 m 6 %, −5–0 m 33–34 %, 0–3 m 55–56 %, 3–6 m 75–76 %, 6–10 m 83–85 %, ≥ 10 m 97–98 % (1–2 % out) | StatsBomb 360, 166 matches (IND-07, `high_balls`) |
| Nearest opponent at the receipt of a completed forward high pass of 30–45 m, by the nearest opponent at release | 0–5 m: 1.0 m (64 % < 2 m); 5–10 m: 2.1 m (48 %); 10–15 m: 5.1 m; ≥ 15 m: 9.0 m (medians) | StatsBomb 360, 166 matches (IND-07, `high_balls`) |
| Open-play pass completion by length and direction (forward / lateral / backward) | 10–20 m 83 / 92 / 97 %; 20–30 m 74 / 91 / 97 %; 30–45 m 55 / 90 / 96 % | StatsBomb open data, 40 matches (`passes` example) |
| Lane of completed forward passes (nearest outfield opponent to 10–85 % of the pass line at release) | ground 10–20 / 20–30 / 30–45 m: median 4.9 / 6.0 / 6.3 m, < 2 m 18 / 11 / 7 %; high: < 2 m 48–67 % | StatsBomb 360, 60 matches (IND-08, `pass_cal`) |
| Ground passes of 20–45 m with a lane < 2 m / 2–4 m, by where the lane defender stands along the line (0.10–0.33 / 0.33–0.60 / 0.60–0.86) | 6.0 / 3.6 / 0.7 % and 10.1 / 7.7 / 1.0 % of completed passes | StatsBomb 360, 60 matches (IND-08, `lane_cal`) |
| Possession xG still to come after a free (pressed) pass receipt, by distance from the own goal line | 0–25 m 0.0097 (0.0057); 25–40 m 0.0105; 40–52.5 m 0.0136 (0.0090); 52.5–65 m 0.0169; 65–75 m 0.0194; 75–85 m 0.0240 (0.0168); 85–95 m 0.037; 95+ m 0.0735 (0.049); pressed / free 0.57–0.78 | StatsBomb open data, 166 matches (value curve, ATT-25; not yet used, see docs/design.md) |
| Running per outfield player (M6) | 10.6–11.4 km per match, high-intensity (> 19.8 km/h) 530–980 m in 30–45 efforts; second half ≈ 7 % below the first; high-intensity running in the last 15 minutes 20–45 % below the first 15 | [DISALVO07; JU23; FIFA-WC22-RUN; BRADLEY09; MOHR03] |
| Scanning before receiving (M6, ATT-05) | 0.44 ± 0.30 scans/s; pass completion 64 / 68 / 81 % after low / medium / high scan frequency | [JORDET20; JORDET13] |
| After a goal (M6) | completion of the conceding team a little lower for ≈ 3–9 minutes; the scoring team's goal rate ≈ its own | [HORRILLO23; HEUER10; GAURIOT18] |
| Fouls, cards, offsides (M7) | per team-match 12.1 fouls (fouler's own / middle / attacking third 17 / 53 / 30 %), 1.64 advantages, 2.12 yellow cards, 0.03 red cards + 0.01 second yellows, 0.11 penalties, 1.49 offsides; tactical fouls ≈ 1.7–3 per match | StatsBomb open data, 40 matches; [DATA-TF01] |
| Set pieces (M7) | per team-match 4.97 corners (0.60 short; a shot within 10 s after 1.61, a goal after 0.11), 0.41 direct free-kick shots (mean xG 0.042), 7.59 goal kicks (3.85 short), 15.9 throw-ins; penalties ≈ 78–80 % scored | StatsBomb open data, 40 matches; [ACAD-PK01; OPTA-PK01] |
| Substitutions and added time (M7) | 4.39 substitutions per team-match, minute p10 / median / p90 51 / 72 / 87, 8 % at half-time; Premier League stoppage time 11:39 per match (2023/24) | StatsBomb open data, 40 matches; [PL-ST24] |

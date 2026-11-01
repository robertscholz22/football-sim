//! The Laws in the match loop (M7) [IFAB]: offside (Law 11), fouls and misconduct (Law 12) with advantage (Law 5.3),
//! penalty kicks (Law 14), substitutions (Law 3) and the allowance for time lost (Law 7.3). The referee's judgements
//! are in `rules::referee`.

use glam::Vec2;

use super::{Advantage, Simulation};
use crate::ai::brain::{InPhase, TeamPhase};
use crate::ai::on_ball::Situation;
use crate::ball::BALL_RADIUS;
use crate::duel::TackleOutcome;
use crate::events::{Card, EventKind};
use crate::match_state::{RestartKind, side_of};
use crate::pitch::{HALF_LENGTH, HALF_WIDTH, Side, Third, third_of};
use crate::player::{Body, Intent, Kinematics, Player, PlayerId, norm};
use crate::rules::referee::{self, Misconduct};

impl Simulation {
    /// Law 11.2: `k` plays the ball. His team-mates in an offside position at this moment are marked (none from a
    /// goal kick, throw-in or corner, Law 11.3); a deliberate play by the other team clears its marks.
    pub(super) fn played(&mut self, k: PlayerId) {
        let exempt = self.restart_kick.take().is_some_and(|r| r.offside_exempt());
        let side = side_of(k);
        for id in 0..self.offside.len() {
            self.offside[id] = side_of(id) == side && id != k && !exempt && referee::offside_position(&self.state, id);
        }
    }

    /// Law 11.2: `id` becomes involved in play (plays the ball or challenges for it) while marked: offside. The pass
    /// is lost; the opponents get an indirect free kick where he was. Returns whether it was given.
    pub(super) fn offside_offence(&mut self, id: PlayerId) -> bool {
        if !self.offside[id] {
            return false;
        }
        let side = side_of(id);
        if let Some(k) = self.pending_pass.take() {
            self.pass_outcome(k, false);
        }
        self.stats.teams[side.index()].offsides += 1;
        self.push(EventKind::Offside { by: id });
        let spot = self.state.players[id].pos;
        self.start_restart(RestartKind::FreeKick { direct: false }, side.opponent(), spot);
        true
    }

    /// LAW-02, Law 12.1: was the challenge by `d` on the carrier `c` a foul? The referee stops play (`Some(true)`) unless the
    /// carrier kept the ball past him in an attacking position and stays on his feet, when he plays advantage
    /// (`Some(false)`, Law 5.3: in the fouled team's attacking half or on a counter-attack, not in the penalty area).
    pub(super) fn challenge_foul(&mut self, d: PlayerId, c: PlayerId, outcome: TackleOutcome, from_behind: bool, now: f32) -> Option<bool> {
        let r = &self.cfg.rules;
        // TRN-04: just after losing the ball, high up the pitch, a beaten player stops the counter with a foul
        // [UEFA-E12; DATA-TF01: within 5 s of the loss, in the last 60 m].
        let tc = &self.cfg.tactics;
        let tactical = (now - self.state.possession_since < tc.trn04_window_s
            && self.state.pos_in(d, side_of(d)).x > HALF_LENGTH - tc.trn04_zone_m)
            .then(|| self.state.team(side_of(d)).tactics.defending.tactical_foul);
        let in_box = referee::in_penalty_area(&self.state, side_of(d), self.state.players[c].pos);
        if !self.rng.chance(referee::foul_chance(&self.state.players[d], outcome, from_behind, tactical, in_box, r)) {
            return None;
        }
        if tactical.is_some() && outcome == TackleOutcome::Beaten && !self.state.players[d].cautioned {
            self.stats.teams[side_of(d).index()].fouls_tactical += 1;
        }
        let misconduct = referee::misconduct(&self.state, r, d, c, from_behind, &mut self.rng);
        let spot = self.state.players[c].pos;
        let fouled = side_of(c);
        let penalty = referee::in_penalty_area(&self.state, side_of(d), spot);
        let attacking = self.state.frame(fouled).to_team(spot).x > 0.0
            || self.state.team_view(fouled).phase == TeamPhase::In(InPhase::CounterAttack);
        let stays = r.stay_min + r.stay_range * norm(self.state.players[c].attrs.strength);
        let advantage = outcome == TackleOutcome::Beaten && !penalty && attacking && self.rng.chance(stays);
        let until = now + r.advantage_s;
        self.record_foul(d, c, penalty, advantage);
        if advantage {
            self.stats.teams[fouled.index()].advantages += 1;
            self.advantage = Some(Advantage { by: d, on: c, spot, misconduct, until });
            Some(false)
        } else {
            self.whistle(d, c, spot, misconduct, now);
            Some(true)
        }
    }

    /// Law 12.1: in an aerial duel the loser `lost` may have fouled the winner (holding, pushing, jumping at him).
    /// Returns whether play was stopped.
    pub(super) fn aerial_foul(&mut self, lost: PlayerId, won: PlayerId, now: f32) -> bool {
        let r = &self.cfg.rules;
        let l = &self.state.players[lost];
        let in_box = referee::in_penalty_area(&self.state, l.side, self.state.players[won].pos);
        let care = if l.cautioned { r.caution_care } else { 1.0 } * if in_box { r.foul_box } else { 1.0 };
        let chance = r.foul_aerial * care * referee::temperament(l, r);
        if !self.rng.chance(chance) {
            return false;
        }
        let misconduct = referee::misconduct(&self.state, r, lost, won, false, &mut self.rng);
        let spot = self.state.players[won].pos;
        let penalty = referee::in_penalty_area(&self.state, side_of(lost), spot);
        self.record_foul(lost, won, penalty, false);
        self.whistle(lost, won, spot, misconduct, now);
        true
    }

    fn record_foul(&mut self, by: PlayerId, on: PlayerId, penalty: bool, advantage: bool) {
        let side = side_of(by);
        let x = self.state.frame(side).to_team(self.state.players[by].pos).x;
        let ts = &mut self.stats.teams[side.index()];
        ts.fouls += 1;
        ts.fouls_by_third[match third_of(x) {
            Third::Defensive => 0,
            Third::Middle => 1,
            Third::Attacking => 2,
        }] += 1;
        self.push(EventKind::Foul { by, on, penalty, advantage });
    }

    /// Stop play for a foul by `by` on `on` at `spot`: the card its misconduct earns, the fouled player down, and a
    /// direct free kick there or a penalty kick if it was in the offender's penalty area (Laws 12–14).
    fn whistle(&mut self, by: PlayerId, on: PlayerId, spot: Vec2, misconduct: Misconduct, now: f32) {
        let offender = side_of(by);
        let penalty = referee::in_penalty_area(&self.state, offender, spot);
        if let Some(card) = misconduct.card(penalty, false) {
            self.caution(by, card);
        }
        let p = &mut self.state.players[on];
        if !p.sent_off {
            p.body = Body::Recovering { until: now + self.cfg.rules.fouled_down_s };
            p.intent = Intent::Position;
        }
        let fouled = offender.opponent();
        if penalty {
            self.stats.teams[fouled.index()].penalties += 1;
            self.allow(self.cfg.rules.added_penalty_s);
            let mark = referee::penalty_mark(&self.state, offender);
            self.start_restart(RestartKind::Penalty, fouled, mark);
        } else {
            self.start_restart(RestartKind::FreeKick { direct: true }, fouled, spot);
        }
    }

    /// Law 5.3: while advantage is played, a loss of the ball by the fouled team before `advantage_s` has passed
    /// brings play back for the foul; after that the advantage has ensued, and a caution or sending-off the foul
    /// earned is given at the next stoppage (an SPA then earns none, a DOGSO a caution).
    pub(super) fn advantage_check(&mut self, now: f32) {
        let Some(a) = self.advantage else { return };
        if self.state.possession != side_of(a.on) {
            self.advantage = None;
            self.whistle(a.by, a.on, a.spot, a.misconduct, now);
        } else if now >= a.until {
            self.advantage = None;
            self.advantage_ensued(a);
        }
    }

    fn advantage_ensued(&mut self, a: Advantage) {
        if let Some(card) = a.misconduct.card(false, true) {
            self.pending_cards.push((a.by, card));
        }
    }

    /// The ball is dead: offside marks lapse, a pending advantage has ensued, deferred cards are shown.
    pub(super) fn stoppage_laws(&mut self) {
        self.offside = [false; 2 * crate::match_state::PLAYERS_PER_TEAM];
        self.restart_kick = None;
        self.set_shot = None;
        self.gk_anticipates = None;
        if let Some(a) = self.advantage.take() {
            self.advantage_ensued(a);
        }
        for (id, card) in std::mem::take(&mut self.pending_cards) {
            if !self.state.players[id].sent_off {
                self.caution(id, card);
            }
        }
    }

    /// Law 12.3: show `card` to `id`; a second caution is a sending-off.
    fn caution(&mut self, id: PlayerId, card: Card) {
        let card = if card == Card::Yellow && self.state.players[id].cautioned { Card::SecondYellow } else { card };
        let ts = &mut self.stats.teams[side_of(id).index()];
        match card {
            Card::Yellow => {
                ts.yellows += 1;
                self.state.players[id].cautioned = true;
            }
            Card::SecondYellow | Card::Red => ts.reds += 1,
        }
        self.push(EventKind::Card { player: id, card });
        self.allow(self.cfg.rules.added_card_s);
        if card != Card::Yellow {
            self.send_off(id);
        }
    }

    /// A sent-off player leaves the field: he waits beyond the touchline at halfway and takes no further part. His
    /// team plays on with the shape it had (it does not reorganise for ten men).
    fn send_off(&mut self, id: PlayerId) {
        if self.state.control == Some(id) {
            self.state.control = None;
        }
        let side = side_of(id);
        let p = &mut self.state.players[id];
        p.sent_off = true;
        p.pos = Vec2::new(if side == Side::Home { -4.0 } else { 4.0 } - 1.0 + p.slot as f32 * 0.2, -(HALF_WIDTH + 3.0));
        p.vel = Vec2::ZERO;
        p.target = p.pos;
        p.target_vel = Vec2::ZERO;
        p.body = Body::Recovering { until: f32::MAX };
        p.intent = Intent::Position;
        self.offside[id] = false;
    }

    /// Law 14: the penalty taker shoots (SP-04).
    pub(super) fn take_penalty(&mut self, taker: PlayerId, spot: Vec2, now: f32) {
        let mut sit = Situation::open_play(taker, spot.extend(BALL_RADIUS));
        sit.restart = true;
        sit.penalty = true;
        self.restart_kick = Some(RestartKind::Penalty);
        self.on_ball_decision(taker, sit, now);
    }

    /// SP-04: the keeper commits to a side (or stays) as the penalty is struck [ACAD-PK01: stays 6 %; ACAD-PK02: dives
    /// to the kicker's natural side 58 % of the time]: guessing right he moves at once (no reaction time); guessing
    /// wrong he is diving the other way.
    pub(super) fn keeper_guess(&mut self, kicker: PlayerId, now: f32) {
        let def = side_of(kicker).opponent();
        let Some(gk) = self.state.squad(def).iter().find(|p| p.is_gk()).map(|p| p.id) else { return };
        let fr = self.state.frame(def);
        let b = self.state.ball;
        let line_x = fr.to_world(Vec2::new(-HALF_LENGTH, 0.0)).x;
        let aim = if b.vel.x.abs() > 0.1 {
            let t = (line_x - b.pos.x) / b.vel.x;
            fr.to_team(Vec2::new(line_x, b.pos.y + b.vel.y * t.max(0.0))).y
        } else {
            0.0
        };
        let tc = &self.cfg.tactics;
        // The kicker's natural side in the keeper's frame: a right-footer's left is the keeper's left (−y).
        let natural = if self.state.players[kicker].attrs.preferred_foot == crate::player::Foot::Right { -1.0 } else { 1.0 };
        let guess: f32 = if self.rng.chance(tc.sp04_keeper_stays) {
            0.0
        } else if self.rng.chance(tc.sp04_keeper_natural) {
            natural
        } else {
            -natural
        };
        let right = if guess == 0.0 { aim.abs() < tc.sp04_centre_m } else { guess * aim > 0.0 };
        if right {
            self.gk_anticipates = Some(gk);
            return;
        }
        let dive = self.cfg.player.gk_dive_speed;
        let g = &mut self.state.players[gk];
        g.body = if guess == 0.0 {
            Body::Recovering { until: now + 0.6 }
        } else {
            Body::Diving { until: now + 0.9, dir: fr.to_world(Vec2::new(0.0, guess)) * dive }
        };
    }

    /// LAW-04, Law 3: at a stoppage each team's default policy makes its substitutions. From `sub_from_min`, once an outfield
    /// player's energy reserve has fallen below a threshold that rises by `sub_reserve_per_min` per minute, the
    /// players within `sub_bundle` of it are replaced, the most tired first (a cautioned player counts `sub_caution`
    /// more tired), at most `sub_per_window` at one stoppage, within the limits on substitutes and stoppages used.
    pub(super) fn substitutions(&mut self) {
        let minute = self.state.clock() / 60.0;
        let r = self.cfg.rules.clone();
        if minute < r.sub_from_min {
            return;
        }
        let threshold = r.sub_reserve + r.sub_reserve_per_min * (minute - r.sub_from_min);
        for side in [Side::Home, Side::Away] {
            let team = &self.state.teams[side.index()];
            if team.subs_used >= r.max_subs || team.sub_windows >= r.max_sub_windows {
                continue;
            }
            let mut tired: Vec<(f32, PlayerId)> = self
                .state
                .squad(side)
                .iter()
                .filter(|p| !p.is_gk() && !p.sent_off)
                .map(|p| (p.reserve - if p.cautioned { r.sub_caution } else { 0.0 }, p.id))
                .filter(|&(v, _)| v < threshold + r.sub_bundle)
                .collect();
            tired.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if tired.first().is_none_or(|t| t.0 >= threshold) {
                continue;
            }
            let room = (r.max_subs - team.subs_used).min(r.sub_per_window) as usize;
            let mut made = 0;
            for &(_, id) in tired.iter().take(room) {
                if self.substitute(id) {
                    made += 1;
                }
            }
            // Half-time substitutions use no opportunity.
            if made > 0 && !(self.state.half == 2 && self.state.half_elapsed() < 1.0) {
                self.state.teams[side.index()].sub_windows += 1;
            }
        }
    }

    /// Replace `id` by the outfield substitute who suits his role best (bench order breaks ties). False if there is
    /// none.
    fn substitute(&mut self, id: PlayerId) -> bool {
        let side = side_of(id);
        let old = &self.state.players[id];
        let roles = [old.role, old.out_role];
        let team = &self.state.teams[side.index()];
        let Some(b) = (0..team.bench.len())
            .filter(|&i| !team.bench[i].is_goalkeeper())
            .max_by(|&i, &j| team.bench[i].suitability(roles[0]).total_cmp(&team.bench[j].suitability(roles[0])).then(j.cmp(&i)))
        else {
            return false;
        };
        let on = self.state.teams[side.index()].bench.remove(b);
        let minute = self.state.clock() / 60.0;
        self.push(EventKind::Substitution { side, off: self.state.players[id].name.clone(), on: on.name.clone() });
        let old = &self.state.players[id];
        let kin = Kinematics::new(&on.attrs, &self.cfg.player, &self.state.conditions, 1.0);
        let mut p = Player::new(
            id,
            side,
            old.slot,
            roles,
            on.name.clone(),
            on.number,
            on.attrs.clone(),
            kin,
            on.instructions.clone(),
            on.confidence,
            roles.map(|r| on.familiarity_with(r)),
        );
        p.concept = self.state.teams[side.index()].practice.concept;
        p.substitute = true;
        p.pos = old.pos;
        p.facing = old.facing;
        p.target = old.target;
        p.beliefs = old.beliefs.clone();
        self.state.players[id] = p;
        let team = &mut self.state.teams[side.index()];
        team.subs_used += 1;
        let ts = &mut self.stats.teams[side.index()];
        ts.subs += 1;
        ts.sub_minutes.push(minute);
        self.allow(self.cfg.rules.added_sub_s);
        true
    }
}

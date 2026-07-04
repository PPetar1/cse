#[cfg(test)]
mod test_support;

use std::fmt::Display;

use rand::Rng;
use time::Date;

use crate::core::State;
use crate::Error;
use crate::core::unit::*;
use crate::core::location::{Location, Terrain};
use crate::procedures::combat::{
    self, BattleOutcome, BattleReport, CombatElement, CombatElementState, SimulationReport,
};

/// Retreat attrition: chance (percent) for each ready element of a retreating
/// unit to end up damaged, and for each damaged element to be lost (captured).
const RETREAT_DAMAGE_CHANCE: f32 = 10.0;
const RETREAT_LOSS_CHANCE: f32 = 25.0;

/// A routing unit whose ready strength has fallen below this fraction of its
/// TOE shatters (disintegrates) when a second roll beats its morale.
const SHATTER_STRENGTH_FRACTION: f32 = 0.5;

/// After a battle every participating element bucket gains
/// `ceil((100 - experience) / EXPERIENCE_GAIN_STEP)` experience: green troops
/// learn fast, veterans have little left to learn, 100 caps itself.
const EXPERIENCE_GAIN_STEP: u32 = 10;

/// Morale settles after a battle: the winning side's buckets gain
/// `ceil((100 - morale) / MORALE_SHIFT_STEP)`, the losing side's lose
/// `ceil(morale / MORALE_SHIFT_STEP)` (routed units lose that twice) —
/// tapering toward the 0/100 bounds just like experience gain.
const MORALE_SHIFT_STEP: u32 = 20;

/// At its faction's turn start every element bucket drifts toward the faction
/// default morale by `ceil(|gap| / MORALE_RECOVERY_STEP)`: battered units
/// recover with rest, battle-euphoric ones settle back down. Gentler than the
/// battle shifts above, so combat outcomes dominate the drift.
const MORALE_RECOVERY_STEP: u32 = 10;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Game {
    pub state: State,
    scenario_name: String,
    players: Vec<Player>,
    turn_system: TurnSystem,
    turn: u32,
    phase: TurnPhase,
    /// The in-game date of the current turn; advances by `turn_length` days
    /// whenever a full turn (every player moved) completes.
    date: Date,
    turn_length: u32,
    victory_conditions: VictoryConditions,
    /// Reinforcements and withdrawals due at a specific turn, applied to the
    /// owning faction's units the moment that turn starts for them.
    scheduled_arrivals: Vec<ScheduledArrival>,
    /// Scenario events: a message plus an optional morale/experience nudge
    /// to a faction's default, due at a specific turn.
    events: Vec<ScenarioEvent>,
    /// Messages from events fired since `run` last drained them via
    /// `take_event_messages` — transient, so it starts empty on load too.
    #[serde(skip)]
    pending_event_messages: Vec<String>,
}

impl Game {
    pub fn build(scenario_toml: String) -> Result<Game, Error> {
       Game::parse_scen_from_toml(scenario_toml) 
    }

    fn parse_scen_from_toml(scenario_toml: String) -> Result<Game, Error>  {
       let mut scenario: Scenario = toml::from_str(&scenario_toml)?;

       if scenario.players.is_empty() {
           return Err(Error::new("The game must have at least 1 player."))
       }

       let players = scenario.players.clone();
       let scenario_name = scenario.name.clone();
       let turn_system = scenario.turn_system;
       let date = scenario.start_date;
       let turn_length = scenario.turn_length;
       let victory_conditions = scenario.victory_conditions.clone();
       // UnitLocationConfig isn't Clone (untagged enums stay minimal), so take
       // these out of the scenario rather than cloning them.
       let reinforcements = std::mem::take(&mut scenario.reinforcements);
       let withdrawals = std::mem::take(&mut scenario.withdrawals);
       let scheduled_arrivals: Vec<ScheduledArrival> = reinforcements.into_iter()
           .chain(withdrawals)
           .map(ScheduledArrival::from)
           .collect();
       let events = scenario.events.clone();

       let state = State::build(scenario)?;

       for hex in &victory_conditions.hexes {
           if state.map.get_location(hex.x, hex.y).is_none() {
               return Err(Error::new(format!(
                   "Victory hex ({}, {}) is not on the map.", hex.x, hex.y,
               )));
           }
       }

       for event in &events {
           if !players.iter().any(|player| player.faction_tag == event.faction) {
               return Err(Error::new(format!(
                   "Event at turn {} references unknown faction '{}'.", event.turn, event.faction,
               )));
           }
       }

       for arrival in &scheduled_arrivals {
           if !state.units.contains_key(&arrival.unit) {
               return Err(Error::new(format!(
                   "Scheduled arrival references unknown unit '{}'.", arrival.unit,
               )));
           }
           match &arrival.location {
               UnitLocation::OnMap(coords) => {
                   if state.map.get_location(coords.x, coords.y).is_none() {
                       return Err(Error::new(format!(
                           "Scheduled arrival for '{}' targets hex ({}, {}) which is not on the map.",
                           arrival.unit, coords.x, coords.y,
                       )));
                   }
               }
               UnitLocation::Offmap(name) => {
                   if state.map.get_offmap_location(name).is_none() {
                       return Err(Error::new(format!(
                           "Scheduled arrival for '{}' targets offmap location '{}' which does not exist.",
                           arrival.unit, name,
                       )));
                   }
               }
           }
       }

       let mut game = Game {
           state,
           scenario_name,
           players,
           turn_system,
           turn: 1,
           phase: TurnPhase { player_on_turn: 0 },
           date,
           turn_length,
           victory_conditions,
           scheduled_arrivals,
           events,
           pending_event_messages: Vec::new(),
       };
       // begin_turn() only fires from end_turn, so the very first player's
       // turn-1 arrivals/events need an explicit first pass here.
       game.apply_scheduled_arrivals();
       game.apply_scheduled_events();

       Ok(game)
    }

    /// End the current player's turn. Under IGO-UGO control passes to the
    /// next player; once every player has moved, the turn counter and the
    /// game date advance. Turn-start effects for the faction coming on turn
    /// (MP reset, morale recovery) hook in here as they land. Returns the
    /// final score once the scenario's `last_turn` has just been completed.
    pub fn end_turn(&mut self) -> Option<VictoryReport> {
        let mut victory = None;
        match self.turn_system {
            TurnSystem::IgoUgo => {
                self.phase.player_on_turn += 1;
                if self.phase.player_on_turn as usize >= self.players.len() {
                    self.phase.player_on_turn = 0;
                    self.turn += 1;
                    self.date += time::Duration::days(self.turn_length.into());
                    if self.victory_conditions.last_turn.is_some_and(|last| self.turn > last) {
                        victory = Some(self.score_victory());
                    }
                }
                self.begin_turn();
            }
        }
        victory
    }

    /// Tally each faction's score: points for victory hexes it holds, points
    /// for the enemy strength it destroyed, minus a penalty for its own
    /// losses — all measured against `State::starting_strength`.
    fn score_victory(&self) -> VictoryReport {
        let scores = self.players.iter().map(|player| {
            let faction = &player.faction_tag;

            let hex_points: f32 = self.victory_conditions.hexes.iter()
                .filter(|hex| self.controlling_faction(hex.x, hex.y).as_deref() == Some(faction.as_str()))
                .map(|hex| hex.points)
                .sum();

            let destruction_points: f32 = self.players.iter()
                .filter(|other| other.faction_tag != *faction)
                .map(|other| {
                    self.percent_destroyed(&other.faction_tag)
                        * self.victory_conditions.points_per_percent_enemy_destroyed
                })
                .sum();

            let loss_penalty = self.percent_destroyed(faction)
                * self.victory_conditions.points_per_percent_own_lost;

            FactionScore {
                faction_name: player.faction_name.clone(),
                hex_points,
                destruction_points,
                loss_penalty,
                total: hex_points + destruction_points - loss_penalty,
            }
        }).collect();

        VictoryReport { scores }
    }

    /// The faction currently holding a hex — the units there, if any (mixed
    /// stacks don't occur: enemy-occupied hexes can't be moved or attacked
    /// into without clearing the defenders first).
    fn controlling_faction(&self, x: u32, y: u32) -> Option<String> {
        let location = self.state.map.get_location(x, y)?;
        Some(self.units_at_location(location).first()?.faction.clone())
    }

    /// Percent of a faction's starting element strength (ready + damaged)
    /// that is now gone — destroyed, shattered or surrendered.
    fn percent_destroyed(&self, faction: &str) -> f32 {
        let starting = *self.state.starting_strength.get(faction).unwrap_or(&0) as f32;
        if starting == 0.0 {
            return 0.0;
        }
        let current: u32 = self.state.units.values()
            .filter(|unit| unit.faction == faction)
            .map(|unit| unit.elements.iter().map(|e| e.ready + e.damaged).sum::<u32>())
            .sum();
        ((starting - current as f32) / starting * 100.0).max(0.0)
    }

    /// Turn-start effects for the faction coming on turn: scheduled
    /// reinforcements/withdrawals and scenario events land first (an event's
    /// morale/experience delta feeds straight into the same turn's drift
    /// target below), then a fresh movement budget from the TOE, and morale
    /// drifting back toward the faction default (rest heals battered units,
    /// euphoria fades).
    fn begin_turn(&mut self) {
        self.apply_scheduled_arrivals();
        self.apply_scheduled_events();

        let player = self.player_on_turn();
        let faction = player.faction_tag.clone();
        let default_morale = player.morale;
        for unit in self.state.units.values_mut() {
            if unit.faction == faction {
                unit.mp_left = self.state.toe.get(&unit.toe).expect("unit's toe vanished").mp;
                for entry in &mut unit.elements {
                    entry.morale = morale_drift(entry.morale, default_morale);
                }
            }
        }
    }

    /// Move every unit whose scheduled arrival falls on the current turn and
    /// belongs to the faction coming on turn — reinforcements step onto the
    /// map, withdrawals step off it, both just a relocation of `location`.
    fn apply_scheduled_arrivals(&mut self) {
        let faction = self.player_on_turn().faction_tag.clone();
        let turn = self.turn;
        for arrival in &self.scheduled_arrivals {
            if arrival.turn != turn {
                continue;
            }
            if let Some(unit) = self.state.units.get_mut(&arrival.unit)
                && unit.faction == faction {
                    unit.location = arrival.location.clone();
                }
        }
    }

    /// Fire every scenario event whose turn falls on the current turn and
    /// whose faction is coming on turn: nudge that faction's default
    /// morale/experience and queue the event's message for `run` to print.
    fn apply_scheduled_events(&mut self) {
        let faction = self.player_on_turn().faction_tag.clone();
        let turn = self.turn;
        for event in &self.events {
            if event.turn != turn || event.faction != faction {
                continue;
            }
            if let Some(player) = self.players.iter_mut().find(|p| p.faction_tag == faction) {
                player.morale = clamp_percent(player.morale as i32 + event.morale_delta);
                player.experience = clamp_percent(player.experience as i32 + event.experience_delta);
            }
            self.pending_event_messages.push(event.message.clone());
        }
    }

    /// Every event message queued since the last time this was called —
    /// `run` drains and prints them after `end_turn`/`new`.
    pub fn take_event_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_event_messages)
    }

    /// Human-readable rundown of every scheduled event: the turn, faction,
    /// message and stat deltas, and whether it has already fired. Backs the
    /// `events` command.
    pub fn event_schedule_summary(&self) -> String {
        if self.events.is_empty() {
            return "No scheduled events.".to_string();
        }
        let mut events: Vec<&ScenarioEvent> = self.events.iter().collect();
        events.sort_by_key(|event| (event.turn, event.faction.clone()));

        let mut out = String::from("Scheduled events:\n");
        for event in events {
            let status = if self.turn >= event.turn { "fired" } else { "pending" };
            out.push_str(&format!(
                "  Turn {} ({}): {} (morale {:+}, experience {:+}) [{}]\n",
                event.turn, event.faction, event.message,
                event.morale_delta, event.experience_delta, status,
            ));
        }
        out.pop();
        out
    }

    /// Human-readable rundown of every scheduled reinforcement/withdrawal:
    /// the turn, unit, destination, and whether it has already happened.
    /// Backs the `reinforcements` command.
    pub fn reinforcement_schedule_summary(&self) -> String {
        if self.scheduled_arrivals.is_empty() {
            return "No scheduled reinforcements or withdrawals.".to_string();
        }
        let mut arrivals: Vec<&ScheduledArrival> = self.scheduled_arrivals.iter().collect();
        arrivals.sort_by_key(|arrival| (arrival.turn, arrival.unit.clone()));

        let mut out = String::from("Scheduled arrivals:\n");
        for arrival in arrivals {
            let destination = match &arrival.location {
                UnitLocation::OnMap(coords) => format!("({}, {})", coords.x, coords.y),
                UnitLocation::Offmap(name) => name.clone(),
            };
            let status = if self.turn >= arrival.turn { "arrived" } else { "pending" };
            out.push_str(&format!(
                "  Turn {}: {} -> {} [{}]\n", arrival.turn, arrival.unit, destination, status,
            ));
        }
        out.pop();
        out
    }

    /// Human-readable rundown of the scenario's victory conditions: the last
    /// turn, each objective hex with its current controller, and the
    /// destruction/loss point multipliers. Backs the `victory` command,
    /// since a scenario's win conditions are otherwise invisible in play.
    pub fn victory_conditions_summary(&self) -> String {
        let mut out = String::new();
        match self.victory_conditions.last_turn {
            Some(last) => out.push_str(&format!("Scenario ends after turn {last}.\n")),
            None => out.push_str("This scenario has no automatic end turn.\n"),
        }
        if self.victory_conditions.hexes.is_empty() {
            out.push_str("No objective hexes.\n");
        } else {
            out.push_str("Objective hexes:\n");
            for hex in &self.victory_conditions.hexes {
                let label = hex.name.as_deref().unwrap_or("(unnamed)");
                let holder = self.controlling_faction(hex.x, hex.y)
                    .unwrap_or_else(|| "nobody".to_string());
                out.push_str(&format!(
                    "  ({}, {}) {}: {:.0} points — held by {}\n",
                    hex.x, hex.y, label, hex.points, holder,
                ));
            }
        }
        out.push_str(&format!(
            "Enemy destruction: {:.1} pts per % of enemy strength destroyed.\n",
            self.victory_conditions.points_per_percent_enemy_destroyed,
        ));
        out.push_str(&format!(
            "Own losses: -{:.1} pts per % of own strength lost.",
            self.victory_conditions.points_per_percent_own_lost,
        ));
        out
    }

    /// Objective hexes for map-view display: coordinates, points and name.
    pub fn victory_hexes(&self) -> Vec<VictoryHexInfo> {
        self.victory_conditions.hexes.iter()
            .map(|hex| VictoryHexInfo { x: hex.x, y: hex.y, points: hex.points, name: hex.name.clone() })
            .collect()
    }

    /// One-line summary of where the game clock stands.
    pub fn status(&self) -> String {
        format!(
            "{} — turn {}, {}. {} to move.",
            self.scenario_name, self.turn, self.date, self.player_on_turn().faction_name,
        )
    }

    fn player_on_turn(&self) -> &Player {
        &self.players[self.phase.player_on_turn as usize]
    }

    pub fn list_units(&self) {
        for unit in self.units_by_name() {
            println!("{}", unit);
        }
    }

    pub fn list_units_detail(&self) {
        for unit in self.units_by_name() {
            println!("{:?}", unit);
        }
    }

    /// All units sorted by name — HashMap iteration order would make the
    /// listing shuffle between runs.
    fn units_by_name(&self) -> Vec<&Unit> {
        let mut units: Vec<&Unit> = self.state.units.values().collect();
        units.sort_by(|a, b| a.name.cmp(&b.name));
        units
    }
    
    /// Units at a location, sorted by name. Sorting matters: the unit index
    /// used by `move_unit` must be stable across calls (HashMap iteration
    /// order is not), and must match what `inspect` shows the player.
    pub fn units_at_location(&self, location: &Location) -> Vec<&Unit> {
        let mut units = Vec::new();
        for unit in self.state.units.values() {
            let units_location = match &unit.location {
                UnitLocation::OnMap(coords) => self.state.map.get_location(coords.x, coords.y),
                UnitLocation::Offmap(name) => self.state.map.get_offmap_location(name),
            };
            if Some(location) == units_location {
                units.push(unit)
            }
        }
        units.sort_by(|a, b| a.name.cmp(&b.name));
        units
    }

    pub fn move_unit(&mut self, x_start: u32, y_start: u32, x_end: u32, y_end: u32, unit_i: usize) -> Result<(), Error> {
        let start = self.state.map.get_location(x_start, y_start).ok_or(Error {
            error_message: "Invalid starting location.".to_string(),
        })?;
        let destination = self.state.map.get_location(x_end, y_end).ok_or(Error {
                error_message: "Invalid destination.".to_string(),
        })?;
        if (x_start, y_start) == (x_end, y_end) {
            return Err(Error::new("The unit is already at the destination."));
        }
        let terrain = destination.terrain;
        if self.state.terrain_costs.cost(terrain).is_none() {
            return Err(Error::new(format!("{terrain:?} is impassable.")));
        }

        // Resolve the order to a unit: units_at_location sorts by name, so
        // the index matches what inspect showed the player.
        let unit = self.units_at_location(start).into_iter().nth(unit_i).ok_or_else(|| Error {
            error_message: format!("No unit with index {} at ({}, {}).", unit_i, x_start, y_start),
        })?;
        let unit_name = unit.name.clone();

        let on_turn = self.player_on_turn().faction_tag.clone();
        if unit.faction != on_turn {
            return Err(Error::new(format!("It is not {}'s turn.", unit.faction)));
        }

        // Taking ground held by the enemy is what `attack` is for — enemy
        // hexes can be neither the destination nor passed through.
        let enemy_hexes: std::collections::HashSet<(u32, u32)> = self.state.units.values()
            .filter(|other| other.faction != on_turn)
            .filter_map(|other| match &other.location {
                UnitLocation::OnMap(coords) => Some((coords.x, coords.y)),
                UnitLocation::Offmap(_) => None,
            })
            .collect();
        if enemy_hexes.contains(&(x_end, y_end)) {
            return Err(Error::new("Cannot move into a hex occupied by the enemy."));
        }

        let cost = self.state.map
            .cheapest_path_cost((x_start, y_start), (x_end, y_end), |coords, location| {
                if enemy_hexes.contains(&coords) {
                    return None;
                }
                self.state.terrain_costs.cost(location.terrain)
            })
            .ok_or_else(|| Error::new("No passable route to the destination."))?;

        let unit = self.state.units.get_mut(&unit_name).expect("moving unit vanished");
        if unit.mp_left < cost {
            return Err(Error::new(format!(
                "Not enough movement points: the way there costs {cost}, {unit_name} has {} left.",
                unit.mp_left,
            )));
        }

        unit.mp_left -= cost;
        unit.location = UnitLocation::OnMap(LocationCoords { x: x_end, y: y_end });

        Ok(())
    }

    /// Resolve an attack by all units in the `from` hex against all units in
    /// the `to` hex. Losses are applied to the units, and a lost defender
    /// retreats to an adjacent hex (or surrenders when there is none).
    pub fn attack(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        rng: &mut impl Rng,
    ) -> Result<AttackReport, Error> {
        let BattlePlan {
            mut attackers,
            mut defenders,
            defender_terrain,
            attacker_names,
            defender_names,
            defender_faction,
        } = self.prepare_battle(from, to)?;

        let battle = combat::resolve_battle(&mut attackers, &mut defenders, defender_terrain, rng);

        // Winners and losers alike learn from standing in a battle — granted
        // before losses and retreats reshape (or remove) the rosters.
        self.apply_experience_gain(&attackers);
        self.apply_experience_gain(&defenders);

        self.apply_battle_losses(&attackers);
        self.apply_battle_losses(&defenders);

        let retreat = if battle.outcome == BattleOutcome::DefenderRetreats {
            self.execute_retreat(from, to, &defender_names, &defender_faction, rng)
        } else {
            Vec::new()
        };

        // A beaten defender always clears its hex (retreated, shattered or
        // surrendered), so the winners advance into it — at no MP cost, the
        // battle already paid for the ground (WitE-style advance after combat).
        let advance = if battle.outcome == BattleOutcome::DefenderRetreats {
            for name in &attacker_names {
                let unit = self.state.units.get_mut(name)
                    .expect("attacking unit vanished mid-attack");
                unit.location = UnitLocation::OnMap(LocationCoords { x: to.0, y: to.1 });
            }
            Some(to)
        } else {
            None
        };

        // Morale settles last, once routs are known: winners rally, losers
        // sag, routed units sag a second time. Morale is collective — every
        // bucket of a participating unit shifts, fought or not — so it works
        // by unit name. (Shattered/surrendered units are gone and skipped.)
        let (winners, losers) = match battle.outcome {
            BattleOutcome::DefenderRetreats => (&attacker_names, &defender_names),
            BattleOutcome::DefenderHolds => (&defender_names, &attacker_names),
        };
        self.apply_morale_shift(winners, true);
        self.apply_morale_shift(losers, false);
        for result in &retreat {
            if let UnitRetreat::Retreated { unit, routed: true, .. } = result
                && let Some(unit) = self.state.units.get_mut(unit) {
                    for entry in &mut unit.elements {
                        entry.morale -= morale_loss(entry.morale);
                    }
                }
        }

        Ok(AttackReport { battle, retreat, advance })
    }

    /// Fight the same attack `runs` times without touching the game state and
    /// report the aggregated outcome/loss distributions — the tuning tool.
    pub fn simulate(
        &self,
        from: (u32, u32),
        to: (u32, u32),
        runs: u32,
        rng: &mut impl Rng,
    ) -> Result<SimulationReport, Error> {
        if runs == 0 {
            return Err(Error::new("Number of battles to simulate must be at least 1."));
        }
        let plan = self.prepare_battle(from, to)?;
        Ok(combat::simulate_battles(&plan.attackers, &plan.defenders, plan.defender_terrain, runs, rng))
    }

    /// Validate an attack order and build the battle snapshots for it.
    /// Shared by `attack` (which then persists results) and `simulate`
    /// (which never does) — both obey the same rules, adjacency and turn
    /// order included, so a simulation is always of a legal attack. Future
    /// order logic that cares about the source hex or whose turn it is
    /// (reserve activation etc.) belongs here too.
    fn prepare_battle(&self, from: (u32, u32), to: (u32, u32)) -> Result<BattlePlan, Error> {
        let from_location = self.state.map.get_location(from.0, from.1)
            .ok_or_else(|| Error::new("Invalid attacking location."))?;
        let to_location = self.state.map.get_location(to.0, to.1)
            .ok_or_else(|| Error::new("Invalid target location."))?;
        if from_location.distance_to(to_location) != Some(1) {
            return Err(Error::new("Attacks can only target an adjacent hex."));
        }

        // Sorted by name (units_at_location), so the snapshot order — and with
        // it a seeded battle — is deterministic despite HashMap storage.
        let attacker_units = self.units_at_location(from_location);
        let defender_units = self.units_at_location(to_location);

        let attacker_faction = single_faction(&attacker_units, "attacking")?;
        let defender_faction = single_faction(&defender_units, "defending")?;
        if attacker_faction == defender_faction {
            return Err(Error::new("Cannot attack units of the same faction."));
        }
        if attacker_faction != self.player_on_turn().faction_tag {
            return Err(Error::new(format!("It is not {attacker_faction}'s turn.")));
        }

        Ok(BattlePlan {
            attackers: combat::combat_elements(&attacker_units, &self.state.elements)?,
            defenders: combat::combat_elements(&defender_units, &self.state.elements)?,
            defender_terrain: to_location.terrain,
            attacker_names: attacker_units.iter().map(|unit| unit.name.clone()).collect(),
            defender_names: defender_units.iter().map(|unit| unit.name.clone()).collect(),
            defender_faction,
        })
    }

    /// Move the beaten defenders out of their hex, with attrition on the way.
    /// All of them go to the same destination; when no valid hex exists the
    /// stack is cut off and surrenders (units removed from the game).
    fn execute_retreat(
        &mut self,
        attacker_hex: (u32, u32),
        defender_hex: (u32, u32),
        defender_names: &[String],
        defender_faction: &str,
        rng: &mut impl Rng,
    ) -> Vec<UnitRetreat> {
        let destination = self.retreat_destination(attacker_hex, defender_hex, defender_faction);

        let mut results = Vec::new();
        for name in defender_names {
            match destination {
                Some((x, y)) => {
                    let unit = self.state.units.get_mut(name)
                        .expect("retreating unit vanished mid-attack");
                    let morale = unit.average_morale() as f32;
                    // Broken morale turns an orderly retreat into a rout:
                    // the attrition rolls happen twice.
                    let routed = rng.random_range(0.0..100.0) >= morale;
                    // A rout can end the unit outright: badly depleted units
                    // that fail a second morale roll disintegrate.
                    let toe = self.state.toe.get(&unit.toe).expect("unit's toe vanished");
                    let shattered = routed
                        && ready_fraction(unit, toe) < SHATTER_STRENGTH_FRACTION
                        && rng.random_range(0.0..100.0) >= morale;
                    if shattered {
                        self.state.units.remove(name);
                        results.push(UnitRetreat::Shattered { unit: name.clone() });
                        continue;
                    }
                    let (mut damaged, mut lost) = retreat_attrition(unit, rng);
                    if routed {
                        let (extra_damaged, extra_lost) = retreat_attrition(unit, rng);
                        damaged += extra_damaged;
                        lost += extra_lost;
                    }
                    unit.location = UnitLocation::OnMap(LocationCoords { x, y });
                    results.push(UnitRetreat::Retreated { unit: name.clone(), to: (x, y), damaged, lost, routed });
                }
                None => {
                    self.state.units.remove(name);
                    results.push(UnitRetreat::Surrendered { unit: name.clone() });
                }
            }
        }
        results
    }

    /// Where a beaten defender goes: an adjacent on-map, non-Water hex free of
    /// enemy units, preferring the one farthest from the attacker. Ties break
    /// on the lowest (x, y) so retreats are deterministic. None = cut off.
    fn retreat_destination(
        &self,
        attacker_hex: (u32, u32),
        defender_hex: (u32, u32),
        defender_faction: &str,
    ) -> Option<(u32, u32)> {
        let attacker_location = self.state.map.get_location(attacker_hex.0, attacker_hex.1)?;
        let defender_location = self.state.map.get_location(defender_hex.0, defender_hex.1)?;

        defender_location.neighbour_coords()
            .into_iter()
            .filter_map(|(x, y)| self.state.map.get_location(x, y).map(|location| ((x, y), location)))
            .filter(|(_, location)| location.terrain != Terrain::Water)
            .filter(|(_, location)| {
                self.units_at_location(location)
                    .iter()
                    .all(|unit| unit.faction == defender_faction)
            })
            .max_by_key(|(coords, location)| {
                (attacker_location.distance_to(location), std::cmp::Reverse(*coords))
            })
            .map(|(coords, _)| coords)
    }

    /// Every element bucket that fielded instances in the battle learns from
    /// it (once per bucket, however many instances fought).
    fn apply_experience_gain(&mut self, elements: &[CombatElement]) {
        let mut seen = std::collections::HashSet::new();
        for element in elements {
            if !seen.insert((&element.unit_name, &element.element_name)) {
                continue;
            }
            if let Some(unit) = self.state.units.get_mut(&element.unit_name)
                && let Some(entry) = unit.elements.iter_mut().find(|e| e.name == element.element_name) {
                    entry.experience +=
                        100u32.saturating_sub(entry.experience).div_ceil(EXPERIENCE_GAIN_STEP);
                }
        }
    }

    /// Post-battle morale for one side's units: winners rally toward 100,
    /// losers sag toward 0. Every bucket of the unit shifts — morale is
    /// collective, unlike the individual experience gain.
    fn apply_morale_shift(&mut self, unit_names: &[String], won: bool) {
        for name in unit_names {
            let Some(unit) = self.state.units.get_mut(name) else {
                continue;
            };
            for entry in &mut unit.elements {
                if won {
                    entry.morale +=
                        100u32.saturating_sub(entry.morale).div_ceil(MORALE_SHIFT_STEP);
                } else {
                    entry.morale -= morale_loss(entry.morale);
                }
            }
        }
    }

    /// Persist battle results: damaged elements move ready → damaged,
    /// destroyed ones are removed for good. Disrupted elements recover and
    /// leave no trace. Each snapshot instance came from one point of `ready`,
    /// so decrementing once per instance cannot underflow.
    fn apply_battle_losses(&mut self, elements: &[CombatElement]) {
        for element in elements {
            let damaged = match element.state {
                CombatElementState::Damaged => true,
                CombatElementState::Destroyed => false,
                CombatElementState::Ready | CombatElementState::Disrupted => continue,
            };
            if let Some(unit) = self.state.units.get_mut(&element.unit_name)
                && let Some(entry) = unit.elements.iter_mut().find(|e| e.name == element.element_name) {
                    entry.ready -= 1;
                    if damaged {
                        entry.damaged += 1;
                    }
                }
        }
    }
}

/// Morale lost from a defeat (or a rout, applied on top): tapers toward 0,
/// and never exceeds the current value, so no clamping is needed.
fn morale_loss(morale: u32) -> u32 {
    morale.div_ceil(MORALE_SHIFT_STEP)
}

/// One turn-start step of morale recovery: toward the faction default from
/// either side, tapering as the gap closes (zero exactly at the default).
fn morale_drift(morale: u32, default: u32) -> u32 {
    if morale < default {
        morale + (default - morale).div_ceil(MORALE_RECOVERY_STEP)
    } else {
        morale - (morale - default).div_ceil(MORALE_RECOVERY_STEP)
    }
}

/// Applies an event's stat delta and keeps the 0-100 range morale/experience
/// are defined over.
fn clamp_percent(value: i32) -> u32 {
    value.clamp(0, 100) as u32
}

/// The unit's ready elements as a fraction of what its TOE prescribes —
/// the strength measure behind the shatter check.
fn ready_fraction(unit: &Unit, toe: &Toe) -> f32 {
    let prescribed: u32 = toe.elements.iter().map(|element| element.amount).sum();
    if prescribed == 0 {
        return 0.0;
    }
    let ready: u32 = unit.elements.iter().map(|element| element.ready).sum();
    ready as f32 / prescribed as f32
}

/// Retreat attrition rolls for one unit: ready elements may end up damaged
/// (RETREAT_DAMAGE_CHANCE), and damaged elements — hard to drag along — may be
/// lost for good (RETREAT_LOSS_CHANCE). Returns (newly damaged, lost).
fn retreat_attrition(unit: &mut Unit, rng: &mut impl Rng) -> (u32, u32) {
    let mut newly_damaged = 0;
    let mut lost = 0;
    for element in &mut unit.elements {
        let mut captured = 0;
        for _ in 0..element.damaged {
            if rng.random_range(0.0..100.0) < RETREAT_LOSS_CHANCE {
                captured += 1;
            }
        }
        element.damaged -= captured;
        lost += captured;

        let mut hurt = 0;
        for _ in 0..element.ready {
            if rng.random_range(0.0..100.0) < RETREAT_DAMAGE_CHANCE {
                hurt += 1;
            }
        }
        element.ready -= hurt;
        element.damaged += hurt;
        newly_damaged += hurt;
    }
    (newly_damaged, lost)
}

/// A validated attack order, ready to fight: the two battle snapshots plus
/// what the game layer needs to persist the aftermath.
struct BattlePlan {
    attackers: Vec<CombatElement>,
    defenders: Vec<CombatElement>,
    defender_terrain: Terrain,
    attacker_names: Vec<String>,
    defender_names: Vec<String>,
    defender_faction: String,
}

/// Everything one attack command did: the battle itself, what the losing
/// defenders had to do afterwards (empty when the defender held), and the
/// hex the attackers advanced into (None when the defender held).
#[derive(Debug)]
pub struct AttackReport {
    pub battle: BattleReport,
    pub retreat: Vec<UnitRetreat>,
    pub advance: Option<(u32, u32)>,
}

#[derive(Debug, PartialEq)]
pub enum UnitRetreat {
    Retreated { unit: String, to: (u32, u32), damaged: u32, lost: u32, routed: bool },
    Shattered { unit: String },
    Surrendered { unit: String },
}

impl Display for AttackReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.battle)?;
        for retreat in &self.retreat {
            write!(f, "\n{}", retreat)?;
        }
        if let Some((x, y)) = self.advance {
            write!(f, "\nAttackers advance into ({}, {})", x, y)?;
        }
        Ok(())
    }
}

impl Display for UnitRetreat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitRetreat::Retreated { unit, to, damaged, lost, routed } => write!(
                f,
                "{} {} to ({}, {}) — retreat losses: {} damaged, {} lost",
                unit,
                if *routed { "routs" } else { "retreats" },
                to.0, to.1, damaged, lost,
            ),
            UnitRetreat::Shattered { unit } => {
                write!(f, "{} routs and shatters — the unit disintegrates!", unit)
            }
            UnitRetreat::Surrendered { unit } => {
                write!(f, "{} has nowhere to retreat and surrenders!", unit)
            }
        }
    }
}

/// The one faction the units on a battle side belong to; errors on an empty
/// side or a mixed stack (multi-faction hexes are unsupported for now).
fn single_faction(units: &[&Unit], side: &str) -> Result<String, Error> {
    let first = units.first()
        .ok_or_else(|| Error::new(format!("No units at the {side} hex.")))?;
    if units.iter().any(|unit| unit.faction != first.faction) {
        return Err(Error::new(format!(
            "Units of multiple factions at the {side} hex are not supported.",
        )));
    }
    Ok(first.faction.clone())
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Player {
    faction_name: String,
    pub faction_tag: String,
    /// Faction-wide default morale/experience, inherited by every element of
    /// the faction's units unless the unit or element sets its own. Lives on
    /// the runtime player so future events can shift it over time.
    #[serde(default = "default_stat")]
    pub morale: u32,
    #[serde(default = "default_stat")]
    pub experience: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct TurnPhase {
    player_on_turn: u32,
}

/// How player turns are sequenced. Scenario-selectable; only IGO-UGO exists
/// today. A future WEGO mode (simultaneous orders, resolved together at turn
/// end) lands as a second variant plus an order queue — the matches on this
/// enum are the places it plugs in.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Deserialize, serde::Serialize)]
pub enum TurnSystem {
    #[default]
    IgoUgo,
}

#[derive(serde::Deserialize)]
pub struct Scenario {
    name: String,
    #[allow(dead_code)]
    game_version: String,
    pub map: String,

    start_date: Date,
    turn_length: u32,
    #[serde(default)]
    turn_system: TurnSystem,
    /// `[terrain_costs]` — MP to enter a hex per terrain name, 0 = impassable.
    /// Anything unlisted falls back to the code defaults
    /// (`Terrain::default_movement_cost`).
    #[serde(default)]
    pub terrain_costs: std::collections::HashMap<Terrain, u32>,

    pub players: Vec<Player>,

    pub toe: Vec<Toe>,

    pub elements: Vec<Element>,

    pub units: Vec<UnitConfig>,

    /// `[victory_conditions]` — optional; a scenario with none never scores
    /// or ends on its own.
    #[serde(default)]
    victory_conditions: VictoryConditions,

    /// `[[reinforcements]]` — units that step onto the map at a scheduled
    /// turn (typically from an offmap box). Mechanically identical to
    /// withdrawals; kept as a separate table only for scenario readability.
    #[serde(default)]
    reinforcements: Vec<ScheduledArrivalConfig>,
    /// `[[withdrawals]]` — units that leave the map (typically back to an
    /// offmap box) at a scheduled turn.
    #[serde(default)]
    withdrawals: Vec<ScheduledArrivalConfig>,

    /// `[[events]]` — a message plus an optional morale/experience nudge to
    /// a faction's default, due at a scheduled turn.
    #[serde(default)]
    events: Vec<ScenarioEvent>,
}

/// One scenario event: at `turn`, `faction`'s default morale/experience
/// shifts by the given deltas (0 = no change either way) and `message`
/// prints. No config/runtime split needed here — unlike locations, nothing
/// about this shape is TOML-only.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ScenarioEvent {
    turn: u32,
    faction: String,
    message: String,
    #[serde(default)]
    morale_delta: i32,
    #[serde(default)]
    experience_delta: i32,
}

/// TOML shape of one scheduled reinforcement or withdrawal entry: move
/// `unit` to `location` the moment `turn` starts for its faction.
#[derive(serde::Deserialize)]
struct ScheduledArrivalConfig {
    unit: String,
    turn: u32,
    location: UnitLocationConfig,
}

/// Runtime form of `ScheduledArrivalConfig` — kept separate the same way
/// `UnitLocation` is kept separate from `UnitLocationConfig`, since postcard
/// save files need this to persist across turns.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ScheduledArrival {
    unit: String,
    turn: u32,
    location: UnitLocation,
}

impl From<ScheduledArrivalConfig> for ScheduledArrival {
    fn from(config: ScheduledArrivalConfig) -> ScheduledArrival {
        ScheduledArrival { unit: config.unit, turn: config.turn, location: config.location.into() }
    }
}

/// How a scenario is won: flat points for holding named hexes at the end,
/// plus points for enemy strength destroyed and a penalty for strength lost
/// (both measured against `State::starting_strength`). `last_turn` is the
/// last turn played; the score is tallied and the scenario ends right after.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
struct VictoryConditions {
    #[serde(default)]
    last_turn: Option<u32>,
    #[serde(default)]
    hexes: Vec<VictoryHex>,
    #[serde(default)]
    points_per_percent_enemy_destroyed: f32,
    #[serde(default)]
    points_per_percent_own_lost: f32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct VictoryHex {
    x: u32,
    y: u32,
    points: f32,
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
}

/// One objective hex, for display outside the game module (the `victory`
/// command's text and the map view's flag markers) — decoupled from the
/// scenario-parsed `VictoryHex` the same way `MapSnapshot` is decoupled from
/// `State`.
#[derive(Debug, Clone)]
pub struct VictoryHexInfo {
    pub x: u32,
    pub y: u32,
    pub points: f32,
    pub name: Option<String>,
}

/// One faction's tally at scenario end.
#[derive(Debug)]
pub struct FactionScore {
    pub faction_name: String,
    pub hex_points: f32,
    pub destruction_points: f32,
    pub loss_penalty: f32,
    pub total: f32,
}

/// The final scores for every faction once a scenario's `last_turn` completes.
#[derive(Debug)]
pub struct VictoryReport {
    pub scores: Vec<FactionScore>,
}

impl VictoryReport {
    /// The sole highest-scoring faction, or None on a tie (a draw).
    pub fn winner(&self) -> Option<&str> {
        let best = self.scores.iter()
            .map(|score| score.total)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut leaders = self.scores.iter().filter(|score| score.total == best);
        let winner = leaders.next()?;
        match leaders.next() {
            None => Some(winner.faction_name.as_str()),
            Some(_) => None,
        }
    }
}

impl Display for VictoryReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Scenario over: final scores ===")?;
        for score in &self.scores {
            // Empty-iterator sums can land on -0.0 (IEEE 754, harmless but
            // ugly to print); +0.0 normalizes the sign for display.
            writeln!(
                f,
                "{}: {:.1} pts (hexes {:.1}, enemy destroyed {:.1}, own losses {:.1})",
                score.faction_name, score.total + 0.0, score.hex_points + 0.0,
                score.destruction_points + 0.0, -score.loss_penalty + 0.0,
            )?;
        }
        match self.winner() {
            Some(name) => write!(f, "{name} wins!"),
            None => write!(f, "The scenario ends in a draw."),
        }
    }
}

#[derive(serde::Deserialize)]
pub struct UnitConfig {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: UnitLocationConfig,
    /// Unit-wide morale/experience, inherited by all its elements. Absent =
    /// the faction default from [[players]].
    pub morale: Option<u32>,
    pub experience: Option<u32>,
    /// Per-element stat overrides ([[units.elements]]), the most specific
    /// setting. Names must exist in the unit's TOE.
    #[serde(default)]
    pub elements: Vec<ElementStatsConfig>,
}

#[derive(serde::Deserialize)]
pub struct ElementStatsConfig {
    pub name: String,
    pub morale: Option<u32>,
    pub experience: Option<u32>,
}

/// Factions that don't specify default morale/experience get an average rating.
fn default_stat() -> u32 {
    50
}

/// Scenario-file form of a unit location. Untagged so the TOML reads naturally:
/// `location = { x = 3, y = 3 }` for a hex, `location = "GE Reserve"` for offmap.
#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum UnitLocationConfig {
    OnMap { x: u32, y: u32 },
    Offmap(String),
}

impl From<UnitLocationConfig> for UnitLocation {
    fn from(config: UnitLocationConfig) -> UnitLocation {
        match config {
            UnitLocationConfig::OnMap { x, y } => UnitLocation::OnMap(LocationCoords { x, y }),
            UnitLocationConfig::Offmap(name) => UnitLocation::Offmap(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn builds_a_game_from_a_minimal_scenario() {
        let game = one_unit_game();

        assert_eq!(game.turn, 1);
        assert_eq!(game.players.len(), 1);
        assert_eq!(game.players[0].faction_tag, "AX");
        assert_eq!(game.state.units.len(), 1);
    }

    #[test]
    fn end_turn_cycles_players_and_advances_the_clock() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Axis to move.");

        game.end_turn();
        // Control passes within the same turn: the clock stands still.
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Soviet Union to move.");

        game.end_turn();
        // Every player has moved: turn and date (turn_length = 7) advance.
        assert_eq!(game.status(), "test scenario — turn 2, 1941-06-29. Axis to move.");
    }

    #[test]
    fn rejects_a_scenario_with_an_invalid_start_date() {
        let scenario = minimal_scenario(ONE_PLAYER, ONMAP_UNIT)
            .replace(r#"start_date = "1941-06-22""#, r#"start_date = "someday""#);

        let error = Game::build(scenario).unwrap_err();
        assert!(error.error_message.contains("start_date"));
    }

    #[test]
    fn rejects_an_unknown_turn_system() {
        let scenario = format!(
            "turn_system = \"Wego\"\n{}",
            minimal_scenario(ONE_PLAYER, ONMAP_UNIT),
        );

        let error = Game::build(scenario).unwrap_err();
        assert!(error.error_message.contains("unknown variant"));
    }

    #[test]
    fn rejects_a_scenario_with_no_players() {
        let error = Game::build(minimal_scenario("players = []", ONMAP_UNIT)).unwrap_err();

        assert!(error.error_message.contains("at least 1 player"));
    }

    #[test]
    fn move_unit_updates_location_and_spends_movement_points() {
        let mut game = one_unit_game();

        // (2, 1) is adjacent Plains: cost 1 from the budget of 16.
        game.move_unit(1, 1, 2, 1, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }));
        assert_eq!(unit.mp_left, 15);
    }

    #[test]
    fn rough_terrain_costs_more_movement_points() {
        let mut game = one_unit_game();

        // (1, 2) is adjacent Forest: cost 2.
        game.move_unit(1, 1, 1, 2, 0).unwrap();

        assert_eq!(game.state.units["1st Test Division"].mp_left, 14);
    }

    #[test]
    fn move_unit_crosses_multiple_hexes_charging_the_path_cost() {
        let mut game = one_unit_game();

        // (1, 1) to (2, 2) is two hexes: cheapest route is two Plains steps
        // via (2, 1), total 2 (the direct Forest neighbour would cost 3).
        game.move_unit(1, 1, 2, 2, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 2 }));
        assert_eq!(unit.mp_left, 14);
    }

    #[test]
    fn pathfinding_routes_around_impassable_terrain() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 3 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // The Water hex (1, 3) sits between (0, 3) and (2, 3): the cheapest
        // way around is three Plains steps via (0, 4) and (1, 4).
        game.move_unit(0, 3, 2, 3, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 3 }));
        assert_eq!(unit.mp_left, 13);
    }

    #[test]
    fn pathfinding_detours_around_enemy_held_hexes() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 3 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 1, y = 4 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();

        // With (1, 4) enemy-held on top of the Water at (1, 3), the 3-point
        // route from the previous test is blocked; the cheapest is now 4.
        game.move_unit(0, 3, 2, 3, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 3 }));
        assert_eq!(unit.mp_left, 12);
    }

    #[test]
    fn move_unit_rejects_an_unreachable_destination() {
        // Plains = 0 leaves only scattered Forest passable: (3, 3) exists
        // and is enterable, but no route reaches it from (1, 2).
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 2 }

[terrain_costs]
Plains = 0
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        let error = game.move_unit(1, 2, 3, 3, 0).unwrap_err();
        assert!(error.error_message.contains("No passable route"));
    }

    #[test]
    fn move_unit_rejects_moving_in_place() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 1, 1, 0).unwrap_err();
        assert!(error.error_message.contains("already at the destination"));
    }

    #[test]
    fn move_unit_rejects_impassable_terrain() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 2 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // (1, 3) is adjacent Water.
        let error = game.move_unit(1, 2, 1, 3, 0).unwrap_err();
        assert!(error.error_message.contains("impassable"));
    }

    #[test]
    fn scenario_terrain_costs_override_the_defaults() {
        // Piggybacks on the units slot to append the scenario-level table.
        let units = format!("{ONMAP_UNIT}\n[terrain_costs]\nForest = 4\nPlains = 0\n");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        // Forest (1, 2) costs the override 4 instead of the default 2.
        game.move_unit(1, 1, 1, 2, 0).unwrap();
        assert_eq!(game.state.units["1st Test Division"].mp_left, 12);

        // 0 makes a terrain impassable: no going back onto the Plains.
        let error = game.move_unit(1, 2, 1, 1, 0).unwrap_err();
        assert!(error.error_message.contains("impassable"));
    }

    #[test]
    fn rejects_an_unknown_terrain_in_terrain_costs() {
        let units = format!("{ONMAP_UNIT}\n[terrain_costs]\nLava = 5\n");

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();
        assert!(error.error_message.contains("unknown variant"));
    }

    #[test]
    fn move_unit_rejects_an_enemy_occupied_destination() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        // The Soviet division sits at (2, 1): entering is an attack, not a move.
        let error = game.move_unit(1, 1, 2, 1, 0).unwrap_err();
        assert!(error.error_message.contains("occupied by the enemy"));
    }

    #[test]
    fn move_unit_allows_stacking_with_friends() {
        let units = r#"
[[units]]
name = "First Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Second Division"
toe = "test_toe"
faction = "AX"
location = { x = 2, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();

        game.move_unit(1, 1, 2, 1, 0).unwrap();

        assert_eq!(
            game.state.units["First Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }),
        );
    }

    #[test]
    fn move_unit_rejects_an_exhausted_unit() {
        let mut game = one_unit_game();
        game.state.units.get_mut("1st Test Division").unwrap().mp_left = 0;

        let error = game.move_unit(1, 1, 2, 1, 0).unwrap_err();
        assert!(error.error_message.contains("Not enough movement points"));
    }

    #[test]
    fn a_factions_movement_points_refill_when_it_comes_on_turn() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        game.move_unit(1, 1, 1, 2, 0).unwrap();
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Soviet turn: the spent Axis budget stays spent.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Axis on turn again: fresh budget from the TOE.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].mp_left, 16);
    }

    #[test]
    fn move_unit_rejects_invalid_start_hex() {
        let mut game = one_unit_game();

        let error = game.move_unit(99, 99, 2, 2, 0).unwrap_err();
        assert!(error.error_message.contains("starting location"));
    }

    #[test]
    fn move_unit_rejects_invalid_destination_hex() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 99, 99, 0).unwrap_err();
        assert!(error.error_message.contains("destination"));
    }

    #[test]
    fn move_unit_rejects_index_with_no_unit() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 2, 1, 5).unwrap_err();
        assert!(error.error_message.contains("index 5"));
        assert!(error.error_message.contains("(1, 1)"));
    }

    #[test]
    fn units_at_location_finds_onmap_and_offmap_units() {
        let units = format!("{ONMAP_UNIT}\n{OFFMAP_UNIT}");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        let hex = game.state.map.get_location(1, 1).unwrap();
        let found = game.units_at_location(hex);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "1st Test Division");

        let reserve = game.state.map.get_offmap_location("GE Reserve").unwrap();
        let found = game.units_at_location(reserve);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Reserve Division");
    }

    #[test]
    fn stacked_units_are_indexed_in_name_order() {
        let units = r#"
[[units]]
name = "Bravo Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Alpha Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Charlie Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        let hex = game.state.map.get_location(1, 1).unwrap();
        let found = game.units_at_location(hex);
        let names: Vec<&str> = found.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(names, ["Alpha Division", "Bravo Division", "Charlie Division"]);

        // Index 1 must address the same unit move_unit sees: Bravo.
        game.move_unit(1, 1, 2, 1, 1).unwrap();
        assert_eq!(
            game.state.units["Bravo Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 })
        );
        assert_eq!(
            game.state.units["Alpha Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 1, y: 1 })
        );
    }

    #[test]
    fn move_unit_rejects_a_unit_of_the_off_turn_faction() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        // Axis moves first: the Soviet division has to wait for its turn.
        let error = game.move_unit(2, 1, 3, 1, 0).unwrap_err();
        assert!(error.error_message.contains("not SU's turn"));

        game.end_turn();
        game.move_unit(2, 1, 3, 1, 0).unwrap();
        assert_eq!(
            game.state.units["Soviet Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 3, y: 1 }),
        );
    }

    #[test]
    fn attack_rejects_the_off_turn_faction() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.attack((2, 1), (1, 1), &mut rng).unwrap_err();
        assert!(error.error_message.contains("not SU's turn"));

        game.end_turn();
        game.attack((2, 1), (1, 1), &mut rng).unwrap();
    }

    #[test]
    fn attack_rejects_a_non_adjacent_target() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 3, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.attack((1, 1), (3, 1), &mut rng).unwrap_err();
        assert!(error.error_message.contains("adjacent"));

        // The tuning tool obeys the same rules — a simulation is always of
        // a legal attack.
        let error = game.simulate((1, 1), (3, 1), 5, &mut rng).unwrap_err();
        assert!(error.error_message.contains("adjacent"));
    }

    #[test]
    fn simulate_rejects_the_off_turn_faction() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.simulate((2, 1), (1, 1), 5, &mut rng).unwrap_err();
        assert!(error.error_message.contains("not SU's turn"));

        game.end_turn();
        game.simulate((2, 1), (1, 1), 5, &mut rng).unwrap();
    }

    #[test]
    fn units_at_location_returns_empty_for_an_empty_hex() {
        let game = one_unit_game();

        let hex = game.state.map.get_location(0, 0).unwrap();
        assert!(game.units_at_location(hex).is_empty());
    }

    #[test]
    fn attack_applies_losses_that_match_the_report() {
        // Two defending units vs one attacker: the defender holds, so no
        // retreat attrition muddies the loss bookkeeping.
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet First"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }

[[units]]
name = "Soviet Second"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert!(!report.battle.rounds.is_empty());
        assert_eq!(report.battle.outcome, BattleOutcome::DefenderHolds);
        assert!(report.retreat.is_empty());
        // A held hex is not entered.
        assert_eq!(report.advance, None);
        assert_eq!(
            game.state.units["Axis Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 1, y: 1 }),
        );
        // The defenders started with 20 ready elements between them; whatever
        // the dice did, the persisted counts must match the report exactly.
        let defenders = [
            &game.state.units["Soviet First"].elements[0],
            &game.state.units["Soviet Second"].elements[0],
        ];
        let damaged: u32 = defenders.iter().map(|e| e.damaged).sum();
        let remaining: u32 = defenders.iter().map(|e| e.ready + e.damaged).sum();
        assert_eq!(damaged, report.battle.defender_losses.damaged);
        assert_eq!(20 - remaining, report.battle.defender_losses.destroyed);
        let attacker = &game.state.units["Axis Division"].elements[0];
        assert_eq!(attacker.damaged, report.battle.attacker_losses.damaged);
        assert_eq!(10 - attacker.ready - attacker.damaged, report.battle.attacker_losses.destroyed);
    }

    #[test]
    fn a_lost_battle_forces_a_retreat_to_an_adjacent_hex() {
        // Three divisions against one: the defender loses and must retreat.
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(100))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        let [UnitRetreat::Retreated { unit, to, routed, .. }] = &report.retreat[..] else {
            panic!("expected exactly one retreated unit, got {:?}", report.retreat);
        };
        assert_eq!(unit, "Soviet Division");
        // Morale 100 never routs.
        assert!(!routed);
        assert_ne!(*to, (1, 1));

        let battle_hex = game.state.map.get_location(2, 1).unwrap();
        let destination = game.state.map.get_location(to.0, to.1)
            .expect("retreat destination must be on the map");
        assert_eq!(battle_hex.distance_to(destination), Some(1));
        assert_ne!(destination.terrain, Terrain::Water);
        assert_eq!(
            game.state.units["Soviet Division"].location,
            UnitLocation::OnMap(LocationCoords { x: to.0, y: to.1 }),
        );
    }

    #[test]
    fn attackers_advance_into_the_vacated_hex_for_free() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(100))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        assert_eq!(report.advance, Some((2, 1)));
        for name in ["Axis First", "Axis Second", "Axis Third"] {
            let unit = &game.state.units[name];
            assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }));
            // Advance after combat costs no movement points.
            assert_eq!(unit.mp_left, 16);
        }
    }

    #[test]
    fn unit_stats_come_from_the_scenario_with_defaults() {
        let units = r#"
[[units]]
name = "Rated Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 80
experience = 65

[[units]]
name = "Unrated Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // Stats live on the elements; a unit-level scenario setting is
        // inherited by all of them.
        let rated = &game.state.units["Rated Division"].elements[0];
        assert_eq!((rated.morale, rated.experience), (80, 65));
        // No unit or faction setting: the default rating.
        let unrated = &game.state.units["Unrated Division"].elements[0];
        assert_eq!((unrated.morale, unrated.experience), (50, 50));
    }

    #[test]
    fn a_defender_with_broken_morale_routs() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(0))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // Morale 0 always routs when forced back — but a unit still near
        // full strength never shatters, it stays in the game.
        assert!(matches!(
            report.retreat[..],
            [UnitRetreat::Retreated { routed: true, .. }],
        ));
        assert!(game.state.units.contains_key("Soviet Division"));
    }

    #[test]
    fn a_routed_understrength_defender_shatters() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(0))).unwrap();
        // Already mauled: 2 of 10 TOE elements ready — far below the shatter
        // threshold. Morale 0 fails both the rout and the shatter roll.
        game.state.units.get_mut("Soviet Division").unwrap().elements[0].ready = 2;
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(
            report.retreat,
            vec![UnitRetreat::Shattered { unit: "Soviet Division".to_string() }],
        );
        assert!(!game.state.units.contains_key("Soviet Division"));
    }

    #[test]
    fn battles_grant_experience_to_both_sides() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // Both start at the default 50: gain is ceil(50 / 10) = 5.
        assert_eq!(game.state.units["Axis Division"].elements[0].experience, 55);
        assert_eq!(game.state.units["Soviet Division"].elements[0].experience, 55);
    }

    #[test]
    fn battles_shift_morale_toward_the_victor() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // 1v1 with this seed the defender holds and wins the battle.
        assert_eq!(report.battle.outcome, BattleOutcome::DefenderHolds);
        // Both start at the default 50; shift is ceil(50 / 20) = 3 each way.
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 53);
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 47);
    }

    #[test]
    fn morale_shifts_stop_at_the_bounds() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 0

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 100
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderHolds);
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 0);
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 100);
    }

    #[test]
    fn morale_shifts_reach_buckets_that_could_not_fight() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 100
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        // The defender has nothing ready to fight with, so it fields no
        // combat elements at all — but morale is collective, and losing the
        // hex must still sag it.
        let bucket = &mut game.state.units.get_mut("Soviet Division").unwrap().elements[0];
        bucket.ready = 0;
        bucket.damaged = 10;
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        // Defeat: 100 - ceil(100/20) = 95; morale 100 never routs.
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 95);
    }

    #[test]
    fn a_routed_unit_loses_morale_twice() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(40))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // With this seed the outnumbered defender is forced back and routs.
        assert!(matches!(
            report.retreat[..],
            [UnitRetreat::Retreated { routed: true, .. }],
        ));
        // Defeat: 40 - ceil(40/20) = 38, then the rout: 38 - ceil(38/20) = 36.
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 36);
    }

    #[test]
    fn morale_drifts_toward_the_faction_default_at_turn_start() {
        // Faction defaults are the unspecified 50; the units start far off it.
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 20

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 90
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();

        // Soviet turn starts: only Soviet morale drifts — down toward 50,
        // 90 - ceil(40 / 10) = 86.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 20);
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 86);

        // Axis turn starts: 20 + ceil(30 / 10) = 23; the Soviets keep 86.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 23);
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 86);
    }

    #[test]
    fn morale_at_the_faction_default_stays_put() {
        // Single player: every end_turn is an Axis turn start. The unit sits
        // at the faction default (50) already.
        let mut game = one_unit_game();

        game.end_turn();
        assert_eq!(game.state.units["1st Test Division"].elements[0].morale, 50);
    }

    #[test]
    fn experience_gain_tapers_off_and_caps_at_100() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
experience = 98

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
experience = 100
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(game.state.units["Axis Division"].elements[0].experience, 99);
        assert_eq!(game.state.units["Soviet Division"].elements[0].experience, 100);
    }

    #[test]
    fn simulate_reports_statistics_without_changing_the_game() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.simulate((1, 1), (2, 1), 25, &mut rng).unwrap();

        assert_eq!(report.runs, 25);
        assert!(report.retreats <= 25);
        // Nothing happened to the real units.
        for name in ["Axis Division", "Soviet Division"] {
            let element = &game.state.units[name].elements[0];
            assert_eq!((element.ready, element.damaged), (10, 0));
        }

        // And the mutable path still works afterwards.
        game.attack((1, 1), (2, 1), &mut rng).unwrap();
    }

    #[test]
    fn simulate_rejects_zero_runs() {
        let game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.simulate((1, 1), (2, 1), 0, &mut rng).unwrap_err();

        assert!(error.error_message.contains("at least 1"));
    }

    #[test]
    fn a_surrounded_defender_surrenders() {
        // Every neighbour of the defender's hex is occupied by the enemy
        // (except Water, which is no escape route anyway): nowhere to go.
        let mut units = String::from(r#"
[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 2 }
"#);
        for i in 0..8 {
            units.push_str(&format!(r#"
[[units]]
name = "Axis Division {i}"
toe = "test_toe"
faction = "AX"
location = "GE Reserve"
"#));
        }
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        let escape_hexes: Vec<(u32, u32)> = game.state.map.get_location(2, 2).unwrap()
            .neighbour_coords()
            .into_iter()
            .filter(|(x, y)| {
                game.state.map.get_location(*x, *y)
                    .is_some_and(|location| location.terrain != Terrain::Water)
            })
            .collect();
        // Three attackers stacked on the first neighbour (to guarantee the
        // battle is lost), one blocker on each remaining one.
        let mut placements = vec![escape_hexes[0]; 3];
        placements.extend(&escape_hexes[1..]);
        for (i, (x, y)) in placements.iter().enumerate() {
            game.state.units.get_mut(&format!("Axis Division {i}")).unwrap().location =
                UnitLocation::OnMap(LocationCoords { x: *x, y: *y });
        }

        let mut rng = StdRng::seed_from_u64(42);
        let report = game.attack(escape_hexes[0], (2, 2), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        assert_eq!(
            report.retreat,
            vec![UnitRetreat::Surrendered { unit: "Soviet Division".to_string() }],
        );
        assert!(!game.state.units.contains_key("Soviet Division"));
    }

    #[test]
    fn attack_rejects_an_empty_source_hex() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // (3, 1) is adjacent to the target but empty — past the adjacency
        // gate, the empty stack is the complaint.
        let error = game.attack((3, 1), (2, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("No units at the attacking hex"));
    }

    #[test]
    fn attack_rejects_attacking_the_same_faction() {
        let units = r#"
[[units]]
name = "First Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Second Division"
toe = "test_toe"
faction = "AX"
location = { x = 2, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.attack((1, 1), (2, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("same faction"));
    }

    #[test]
    fn end_turn_never_scores_without_a_last_turn() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        assert!(game.end_turn().is_none());
        assert!(game.end_turn().is_none());
        assert!(game.end_turn().is_none());
        assert!(game.end_turn().is_none());
    }

    #[test]
    fn victory_score_awards_points_for_controlled_hexes() {
        let victory_conditions = r#"
[victory_conditions]
last_turn = 1

[[victory_conditions.hexes]]
x = 1
y = 1
points = 10

[[victory_conditions.hexes]]
x = 2
y = 1
points = 20
"#;
        let units = format!("{OPPOSING_UNITS}\n{victory_conditions}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        // Turn 1 is not over yet: no score.
        assert!(game.end_turn().is_none());
        // Every player has now moved in turn 1: last_turn is complete.
        let report = game.end_turn().unwrap();

        let axis = report.scores.iter().find(|s| s.faction_name == "Axis").unwrap();
        assert_eq!(axis.hex_points, 10.0);
        let soviet = report.scores.iter().find(|s| s.faction_name == "Soviet Union").unwrap();
        assert_eq!(soviet.hex_points, 20.0);
        assert_eq!(report.winner(), Some("Soviet Union"));
    }

    #[test]
    fn victory_score_rewards_enemy_destruction_and_penalizes_own_losses() {
        let victory_conditions = r#"
[victory_conditions]
last_turn = 1
points_per_percent_enemy_destroyed = 2.0
points_per_percent_own_lost = 1.0
"#;
        let units = format!("{OPPOSING_UNITS}\n{victory_conditions}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();
        // The Soviet division starts with 10 ready test_element instances;
        // half of them are gone.
        game.state.units.get_mut("Soviet Division").unwrap().elements[0].ready = 5;

        game.end_turn();
        let report = game.end_turn().unwrap();

        let axis = report.scores.iter().find(|s| s.faction_name == "Axis").unwrap();
        // 50% of the Soviets destroyed, times the 2.0 multiplier.
        assert_eq!(axis.destruction_points, 100.0);
        assert_eq!(axis.loss_penalty, 0.0);
        assert_eq!(axis.total, 100.0);

        let soviet = report.scores.iter().find(|s| s.faction_name == "Soviet Union").unwrap();
        assert_eq!(soviet.destruction_points, 0.0);
        // 50% of their own strength lost, times the 1.0 penalty multiplier.
        assert_eq!(soviet.loss_penalty, 50.0);
        assert_eq!(soviet.total, -50.0);

        assert_eq!(report.winner(), Some("Axis"));
    }

    #[test]
    fn a_tied_score_is_a_draw() {
        let victory_conditions = "\n[victory_conditions]\nlast_turn = 1\n";
        let units = format!("{OPPOSING_UNITS}\n{victory_conditions}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.end_turn();
        let report = game.end_turn().unwrap();

        assert_eq!(report.winner(), None);
    }

    #[test]
    fn rejects_a_victory_hex_outside_the_map() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[victory_conditions]
last_turn = 5

[[victory_conditions.hexes]]
x = 999
y = 999
points = 10
"#;
        let error = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap_err();

        assert!(error.error_message.contains("not on the map"));
    }

    #[test]
    fn a_reinforcement_scheduled_for_turn_one_arrives_immediately() {
        // begin_turn() only fires from end_turn, so turn-1 arrivals for the
        // first-moving player need to be applied right at Game::build.
        let units = format!(
            "{OFFMAP_UNIT}\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 1\nlocation = {{ x = 2, y = 2 }}\n"
        );
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 2 }),
        );
    }

    #[test]
    fn a_reinforcement_arrives_only_on_its_scheduled_turn() {
        let units = format!(
            "{OFFMAP_UNIT}\n\n[[units]]\nname = \"Soviet Division\"\ntoe = \"test_toe\"\nfaction = \"SU\"\nlocation = {{ x = 2, y = 1 }}\n\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 4, y = 4 }}\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        // Turn 1, Axis to move: not yet the scheduled turn.
        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );

        game.end_turn(); // Axis -> Soviet, still turn 1.
        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );

        game.end_turn(); // Soviet -> Axis, turn becomes 2: the reinforcement lands.
        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 4, y: 4 }),
        );
    }

    #[test]
    fn a_withdrawal_moves_a_unit_back_offmap() {
        let units = format!(
            "{OPPOSING_UNITS}\n[[withdrawals]]\nunit = \"Axis Division\"\nturn = 2\nlocation = \"GE Reserve\"\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.end_turn(); // turn 1, Soviet.
        game.end_turn(); // turn 2, Axis: the withdrawal fires.

        assert_eq!(
            game.state.units["Axis Division"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );
    }

    #[test]
    fn rejects_a_reinforcement_for_an_unknown_unit() {
        let units = format!(
            "{ONMAP_UNIT}\n[[reinforcements]]\nunit = \"Ghost Division\"\nturn = 2\nlocation = {{ x = 2, y = 2 }}\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("Ghost Division"));
    }

    #[test]
    fn rejects_a_reinforcement_targeting_a_hex_outside_the_map() {
        let units = format!(
            "{OFFMAP_UNIT}\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 999, y = 999 }}\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("not on the map"));
    }

    #[test]
    fn rejects_a_withdrawal_targeting_an_unknown_offmap_location() {
        let units = format!(
            "{ONMAP_UNIT}\n[[withdrawals]]\nunit = \"1st Test Division\"\nturn = 2\nlocation = \"Nowhere\"\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("Nowhere"));
    }

    #[test]
    fn reinforcement_schedule_summary_tracks_arrival_status() {
        let units = format!(
            "{OFFMAP_UNIT}\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 2, y = 2 }}\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert!(game.reinforcement_schedule_summary().contains("pending"));

        game.end_turn(); // ONE_PLAYER: every end_turn completes a full round.

        assert!(game.reinforcement_schedule_summary().contains("arrived"));
    }

    #[test]
    fn a_turn_one_event_fires_immediately_and_shifts_faction_morale() {
        // begin_turn() only fires from end_turn, so turn-1 events for the
        // first-moving player need the same explicit first pass as
        // reinforcements.
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 1\nfaction = \"AX\"\nmessage = \"Opening barrage\"\nmorale_delta = 10\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(game.players[0].morale, 60); // default 50 + 10
        assert_eq!(game.take_event_messages(), vec!["Opening barrage".to_string()]);
    }

    #[test]
    fn an_event_fires_only_for_its_faction_on_its_scheduled_turn() {
        let units = format!(
            "{OPPOSING_UNITS}\n[[events]]\nturn = 2\nfaction = \"SU\"\nmessage = \"Reinforcement convoy arrives\"\nmorale_delta = 5\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        assert!(game.take_event_messages().is_empty());

        game.end_turn(); // Axis -> Soviet, still turn 1: not yet scheduled.
        assert!(game.take_event_messages().is_empty());

        game.end_turn(); // Soviet -> Axis, turn becomes 2: Axis's start, wrong faction.
        assert!(game.take_event_messages().is_empty());

        game.end_turn(); // Axis -> Soviet, turn stays 2: the Soviet event fires.
        assert_eq!(game.take_event_messages(), vec!["Reinforcement convoy arrives".to_string()]);
    }

    #[test]
    fn event_morale_delta_clamps_to_the_valid_range() {
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 1\nfaction = \"AX\"\nmessage = \"Catastrophe\"\nmorale_delta = -1000\n"
        );
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(game.players[0].morale, 0);
    }

    #[test]
    fn an_events_morale_delta_feeds_the_same_turns_drift_target() {
        // The unit's element sits at 20, far below the unspecified default
        // of 50 — it would drift up toward 50 on an ordinary turn start. An
        // event bumping the default to 90 on the same turn should make the
        // drift aim at 90 instead, since events apply before the drift.
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 2\nfaction = \"AX\"\nmessage = \"Big win\"\nmorale_delta = 40\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();
        game.state.units.get_mut("1st Test Division").unwrap().elements[0].morale = 20;

        game.end_turn(); // turn becomes 2: the event fires (default 50 -> 90), then drift runs.

        // 20 + ceil((90 - 20) / 10) = 27.
        assert_eq!(game.state.units["1st Test Division"].elements[0].morale, 27);
    }

    #[test]
    fn rejects_an_event_for_an_unknown_faction() {
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 2\nfaction = \"ZZ\"\nmessage = \"Ghost event\"\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("ZZ"));
    }

    #[test]
    fn event_schedule_summary_tracks_fired_status() {
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 2\nfaction = \"AX\"\nmessage = \"Spring thaw\"\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert!(game.event_schedule_summary().contains("pending"));

        game.end_turn(); // ONE_PLAYER: every end_turn completes a full round.

        assert!(game.event_schedule_summary().contains("fired"));
    }

    #[test]
    fn builds_the_real_basic_scenario() {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/basic_scenario.scen"),
        ).unwrap();
        let game = Game::build(contents).unwrap();

        assert_eq!(game.players.len(), 2);
        assert_eq!(game.state.units.len(), 8);
        // Guards the TOE/element referential integrity of the shipped scenario.
        assert!(game.state.elements.contains_key("SU_45mm_at_gun"));
        // Morale/experience inheritance: the 101st takes the Soviet faction
        // defaults, except its howitzer crews' experience override.
        let infantry = &game.state.units["101st Infantry division"];
        assert_eq!(infantry.mp_left, 16);
        let squads = infantry.elements.iter().find(|e| e.name == "SU_inf_squad").unwrap();
        assert_eq!((squads.morale, squads.experience), (45, 35));
        let howitzers = infantry.elements.iter()
            .find(|e| e.name == "SU_122mm_howitzer_M1938").unwrap();
        assert_eq!((howitzers.morale, howitzers.experience), (45, 55));
    }

    #[test]
    fn builds_the_real_frontline_sector_scenario() {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/frontline_sector.scen"),
        ).unwrap();
        let mut game = Game::build(contents).unwrap();

        assert_eq!(game.players.len(), 2);
        // 10 Soviet frontline + 2 reserve, 8 German infantry + 2 Panzer on
        // the line + 1 Panzer in reserve.
        assert_eq!(game.state.units.len(), 23);
        // Guards the TOE/element referential integrity of the shipped scenario.
        assert!(game.state.elements.contains_key("GE_37mm_pak"));
        // The continuous Soviet line: every hex from (0, 4) to (9, 4) is held.
        for x in 0..10 {
            let location = game.state.map.get_location(x, 4).unwrap();
            assert_eq!(game.units_at_location(location).first().unwrap().faction, "SU");
        }
        // Turn-1 event already fired (see Game::build's explicit first pass).
        assert_eq!(
            game.take_event_messages(),
            vec!["The assault opens with total surprise; German morale surges.".to_string()],
        );
        assert_eq!(game.event_schedule_summary().matches("pending").count(), 2);
        assert_eq!(game.reinforcement_schedule_summary().matches("pending").count(), 4);
    }
}

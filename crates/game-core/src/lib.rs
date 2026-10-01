//! Deterministic, synchronous rules for the local campus recycling match.
//! I/O and bot execution belong to the runner, never to this crate.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const WIDTH: u8 = 15;
pub const HEIGHT: u8 = 15;
pub const CAPACITY: u8 = 4;
pub const MAX_TURNS: u16 = 200;
pub const TOTAL_RESOURCES: u16 = 96;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Team {
    A,
    B,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Position {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Resource {
    pub x: u8,
    pub y: u8,
    pub amount: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Robot {
    pub id: String,
    pub team: Team,
    pub x: u8,
    pub y: u8,
    pub cargo: u8,
}

impl Robot {
    fn position(&self) -> Position {
        Position {
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Scores {
    #[serde(rename = "A")]
    pub a: u16,
    #[serde(rename = "B")]
    pub b: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    AllDelivered,
    TurnLimit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Outcome {
    /// `None` denotes a draw; cargo is never used as a tie-breaker.
    pub winner: Option<Team>,
    pub reason: EndReason,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Command {
    pub robot_id: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameState {
    pub width: u8,
    pub height: u8,
    pub capacity: u8,
    pub max_turns: u16,
    pub total_resources: u16,
    pub completed_turn: u16,
    pub resources: Vec<Resource>,
    pub obstacles: Vec<Position>,
    pub robots: Vec<Robot>,
    pub scores: Scores,
    pub outcome: Option<Outcome>,
}

#[derive(Clone, Copy)]
enum Action {
    Wait,
    Pick,
    Move(i16, i16),
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        let left = [
            (3, 2),
            (3, 7),
            (3, 12),
            (4, 4),
            (4, 10),
            (5, 3),
            (5, 7),
            (5, 11),
            (6, 6),
        ];
        let mut resources = Vec::with_capacity(24);
        for (x, y) in left {
            resources.push(Resource { x, y, amount: 4 });
            resources.push(Resource {
                x: WIDTH - 1 - x,
                y,
                amount: 4,
            });
        }
        for y in [2, 4, 6, 8, 10, 12] {
            resources.push(Resource { x: 7, y, amount: 4 });
        }
        let mut robots = Vec::with_capacity(6);
        for (team, prefix, x) in [(Team::A, "A", 0), (Team::B, "B", 14)] {
            for (index, y) in [3, 7, 11].into_iter().enumerate() {
                robots.push(Robot {
                    id: format!("{prefix}{index}"),
                    team,
                    x,
                    y,
                    cargo: 0,
                });
            }
        }
        Self {
            width: WIDTH,
            height: HEIGHT,
            capacity: CAPACITY,
            max_turns: MAX_TURNS,
            total_resources: TOTAL_RESOURCES,
            completed_turn: 0,
            resources,
            obstacles: Vec::new(),
            robots,
            scores: Scores::default(),
            outcome: None,
        }
    }

    /// Apply one complete simultaneous turn. Both arrays refer to this same
    /// starting snapshot. A rejected turn leaves the state unchanged.
    pub fn step(&mut self, a: &[Command], b: &[Command]) -> Result<(), String> {
        self.validate()?;
        if self.outcome.is_some() || self.completed_turn >= self.max_turns {
            return Err("The match has already finished".to_owned());
        }

        let actions: Vec<Action> = self
            .robots
            .iter()
            .map(|robot| {
                let commands = match robot.team {
                    Team::A => a,
                    Team::B => b,
                };
                // Count before parsing: even an invalid second command makes
                // the whole robot WAIT, regardless of array order.
                let mut own = commands.iter().filter(|c| c.robot_id == robot.id);
                let first = own.next();
                if own.next().is_some() {
                    return Action::Wait;
                }
                first.map_or(Action::Wait, normalize)
            })
            .collect();

        let occupied: HashSet<Position> = self.robots.iter().map(Robot::position).collect();
        let blocked: HashSet<Position> = self.obstacles.iter().copied().collect();
        let candidates: Vec<Option<Position>> = self
            .robots
            .iter()
            .zip(&actions)
            .map(|(robot, action)| {
                let Action::Move(dx, dy) = action else {
                    return None;
                };
                let x = i16::from(robot.x) + dx;
                let y = i16::from(robot.y) + dy;
                if x < 0 || y < 0 || x >= i16::from(self.width) || y >= i16::from(self.height) {
                    return None;
                }
                let destination = Position {
                    x: x as u8,
                    y: y as u8,
                };
                if enemy_depot(robot.team, destination.x, self.width)
                    || occupied.contains(&destination)
                    || blocked.contains(&destination)
                {
                    return None;
                }
                Some(destination)
            })
            .collect();
        let mut destination_counts: HashMap<Position, usize> = HashMap::new();
        for destination in candidates.iter().flatten() {
            *destination_counts.entry(*destination).or_default() += 1;
        }

        for (index, robot) in self.robots.iter_mut().enumerate() {
            if let Some(destination) = candidates[index] {
                if destination_counts[&destination] == 1 {
                    robot.x = destination.x;
                    robot.y = destination.y;
                }
            }
            if matches!(actions[index], Action::Pick) {
                if let Some(resource) = self
                    .resources
                    .iter_mut()
                    .find(|resource| resource.x == robot.x && resource.y == robot.y)
                {
                    let amount = resource.amount.min(self.capacity - robot.cargo);
                    robot.cargo += amount;
                    resource.amount -= amount;
                }
            }
        }

        for robot in &mut self.robots {
            let own_depot = match robot.team {
                Team::A => robot.x == 0,
                Team::B => robot.x == self.width - 1,
            };
            if own_depot {
                match robot.team {
                    Team::A => self.scores.a += u16::from(robot.cargo),
                    Team::B => self.scores.b += u16::from(robot.cargo),
                }
                robot.cargo = 0;
            }
        }
        self.completed_turn += 1;
        let reason = if self.scores.a + self.scores.b == self.total_resources {
            Some(EndReason::AllDelivered)
        } else if self.completed_turn == self.max_turns {
            Some(EndReason::TurnLimit)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.outcome = Some(Outcome {
                winner: match self.scores.a.cmp(&self.scores.b) {
                    std::cmp::Ordering::Greater => Some(Team::A),
                    std::cmp::Ordering::Less => Some(Team::B),
                    std::cmp::Ordering::Equal => None,
                },
                reason,
            });
        }
        self.validate()
    }

    /// Check the resource ledger and legal robot/terrain state. This also makes
    /// malformed states fail explicitly before any turn can be applied.
    pub fn validate(&self) -> Result<(), String> {
        if self.width != WIDTH
            || self.height != HEIGHT
            || self.capacity != CAPACITY
            || self.max_turns != MAX_TURNS
            || self.total_resources != TOTAL_RESOURCES
        {
            return Err("Game constants do not match the fixed MVP rules".to_owned());
        }
        if self.completed_turn > self.max_turns {
            return Err("Completed turn exceeds the turn limit".to_owned());
        }
        let inside = |p: Position| p.x < self.width && p.y < self.height;
        let mut resource_positions = HashSet::new();
        for resource in &self.resources {
            let position = Position {
                x: resource.x,
                y: resource.y,
            };
            if !inside(position)
                || resource.x == 0
                || resource.x == self.width - 1
                || resource.amount > CAPACITY
                || !resource_positions.insert(position)
            {
                return Err("Invalid resource cell".to_owned());
            }
        }
        let mut blocked = HashSet::new();
        for obstacle in &self.obstacles {
            if !inside(*obstacle)
                || obstacle.x == 0
                || obstacle.x == self.width - 1
                || resource_positions.contains(obstacle)
                || !blocked.insert(*obstacle)
            {
                return Err("Invalid obstacle cell".to_owned());
            }
        }
        if self.robots.len() != 6 {
            return Err("There must be exactly six robots".to_owned());
        }
        let mut ids = HashSet::new();
        let mut occupied = HashSet::new();
        for robot in &self.robots {
            let expected_prefix = match robot.team {
                Team::A => 'A',
                Team::B => 'B',
            };
            if !matches!(robot.id.as_bytes(), [prefix, b'0'..=b'2'] if *prefix == expected_prefix as u8)
                || !ids.insert(robot.id.as_str())
                || !inside(robot.position())
                || robot.cargo > self.capacity
                || enemy_depot(robot.team, robot.x, self.width)
                || blocked.contains(&robot.position())
                || !occupied.insert(robot.position())
            {
                return Err(format!("Invalid robot state: {}", robot.id));
            }
        }
        let remaining: u32 = self.resources.iter().map(|r| u32::from(r.amount)).sum();
        let cargo: u32 = self.robots.iter().map(|r| u32::from(r.cargo)).sum();
        if remaining + cargo + u32::from(self.scores.a) + u32::from(self.scores.b)
            != u32::from(self.total_resources)
        {
            return Err("Resource conservation violated".to_owned());
        }
        if let Some(outcome) = self.outcome {
            let winner = match self.scores.a.cmp(&self.scores.b) {
                std::cmp::Ordering::Greater => Some(Team::A),
                std::cmp::Ordering::Less => Some(Team::B),
                std::cmp::Ordering::Equal => None,
            };
            let valid_reason = match outcome.reason {
                EndReason::AllDelivered => self.scores.a + self.scores.b == self.total_resources,
                EndReason::TurnLimit => self.completed_turn == self.max_turns,
            };
            if outcome.winner != winner || !valid_reason {
                return Err("Outcome does not match the final state".to_owned());
            }
        }
        Ok(())
    }
}

fn normalize(command: &Command) -> Action {
    match (command.action.as_str(), command.direction.as_deref()) {
        ("WAIT", None) => Action::Wait,
        ("PICK", None) => Action::Pick,
        ("MOVE", Some("UP")) => Action::Move(0, -1),
        ("MOVE", Some("DOWN")) => Action::Move(0, 1),
        ("MOVE", Some("LEFT")) => Action::Move(-1, 0),
        ("MOVE", Some("RIGHT")) => Action::Move(1, 0),
        _ => Action::Wait,
    }
}

fn enemy_depot(team: Team, x: u8, width: u8) -> bool {
    match team {
        Team::A => x == width - 1,
        Team::B => x == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn command(id: &str, action: &str, direction: Option<&str>) -> Command {
        Command {
            robot_id: id.to_owned(),
            action: action.to_owned(),
            direction: direction.map(str::to_owned),
        }
    }

    fn movement(id: &str, direction: &str) -> Command {
        command(id, "MOVE", Some(direction))
    }

    fn pick(id: &str) -> Command {
        command(id, "PICK", None)
    }

    fn robot<'a>(state: &'a GameState, id: &str) -> &'a Robot {
        state.robots.iter().find(|r| r.id == id).unwrap()
    }

    fn robot_mut<'a>(state: &'a mut GameState, id: &str) -> &'a mut Robot {
        state.robots.iter_mut().find(|r| r.id == id).unwrap()
    }

    fn place(state: &mut GameState, id: &str, x: u8, y: u8) {
        let robot = robot_mut(state, id);
        robot.x = x;
        robot.y = y;
    }

    fn cargo(state: &mut GameState, id: &str, amount: u8) {
        assert_eq!(robot(state, id).cargo, 0);
        let mut pending = amount;
        for resource in &mut state.resources {
            let taken = resource.amount.min(pending);
            resource.amount -= taken;
            pending -= taken;
        }
        assert_eq!(pending, 0);
        robot_mut(state, id).cargo = amount;
    }

    fn depleted() -> GameState {
        let mut state = GameState::new();
        for resource in &mut state.resources {
            resource.amount = 0;
        }
        state.scores.a = TOTAL_RESOURCES;
        state
    }

    fn canonical(mut state: GameState) -> GameState {
        state.robots.sort_by(|a, b| a.id.cmp(&b.id));
        state.resources.sort_by_key(|r| (r.x, r.y));
        state.obstacles.sort_by_key(|p| (p.x, p.y));
        state
    }

    #[test]
    fn initial_fixed_map_has_correct_counts_symmetry_and_reachable_resources() {
        let state = GameState::new();
        assert_eq!(state, GameState::default());
        assert_eq!((state.width, state.height), (15, 15));
        assert!(state.obstacles.is_empty());
        assert_eq!(state.resources.len(), 24);
        assert!(state.resources.iter().all(|r| r.amount == 4));
        assert_eq!(state.resources.iter().filter(|r| r.x == 7).count(), 6);
        assert_eq!(state.resources.iter().filter(|r| r.x < 7).count(), 9);
        assert_eq!(state.resources.iter().filter(|r| r.x > 7).count(), 9);
        for resource in &state.resources {
            assert!(state.resources.iter().any(|other| {
                other.x == 14 - resource.x && other.y == resource.y && other.amount == 4
            }));
        }
        for (index, y) in [3, 7, 11].into_iter().enumerate() {
            let a = robot(&state, &format!("A{index}"));
            let b = robot(&state, &format!("B{index}"));
            assert_eq!((a.x, a.y, a.team, a.cargo), (0, y, Team::A, 0));
            assert_eq!((b.x, b.y, b.team, b.cargo), (14, y, Team::B, 0));
        }
        assert_eq!(state.completed_turn, 0);
        assert_eq!(state.scores, Scores::default());
        assert!(state.outcome.is_none());
        for team in [Team::A, Team::B] {
            let start = robot(&state, if team == Team::A { "A0" } else { "B0" }).position();
            let mut reached = HashSet::from([start]);
            let mut queue = VecDeque::from([start]);
            while let Some(position) = queue.pop_front() {
                for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                    let x = i16::from(position.x) + dx;
                    let y = i16::from(position.y) + dy;
                    if !(0..15).contains(&x) || !(0..15).contains(&y) {
                        continue;
                    }
                    let next = Position {
                        x: x as u8,
                        y: y as u8,
                    };
                    if !enemy_depot(team, next.x, 15) && reached.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
            assert!(state
                .resources
                .iter()
                .all(|r| reached.contains(&Position { x: r.x, y: r.y })));
        }
        state.validate().unwrap();
    }

    #[test]
    fn normal_move_preserves_cargo_and_does_not_collect() {
        let mut state = GameState::new();
        place(&mut state, "A0", 4, 3);
        cargo(&mut state, "A0", 2);
        let target_before = state
            .resources
            .iter()
            .find(|r| (r.x, r.y) == (5, 3))
            .unwrap()
            .amount;
        state.step(&[movement("A0", "RIGHT")], &[]).unwrap();
        assert_eq!((robot(&state, "A0").x, robot(&state, "A0").y), (5, 3));
        assert_eq!(robot(&state, "A0").cargo, 2);
        assert_eq!(
            state
                .resources
                .iter()
                .find(|r| (r.x, r.y) == (5, 3))
                .unwrap()
                .amount,
            target_before
        );
        assert_eq!(state.scores, Scores::default());
        assert_eq!(state.completed_turn, 1);
    }

    #[test]
    fn all_map_edges_reject_moves_without_wrapping() {
        let mut state = GameState::new();
        place(&mut state, "A1", 4, 0);
        place(&mut state, "B1", 10, 14);
        let before = state.robots.clone();
        state
            .step(
                &[movement("A0", "LEFT"), movement("A1", "UP")],
                &[movement("B0", "RIGHT"), movement("B1", "DOWN")],
            )
            .unwrap();
        assert_eq!(state.robots, before);
    }

    #[test]
    fn obstacles_and_both_enemy_depots_reject_moves() {
        let mut state = GameState::new();
        place(&mut state, "A0", 2, 3);
        place(&mut state, "A1", 13, 5);
        place(&mut state, "B0", 1, 6);
        state.obstacles.push(Position { x: 3, y: 3 });
        let before = state.robots.clone();
        state
            .step(
                &[movement("A0", "RIGHT"), movement("A1", "RIGHT")],
                &[movement("B0", "LEFT")],
            )
            .unwrap();
        assert_eq!(state.robots, before);
    }

    #[test]
    fn entering_stationary_or_vacated_start_cell_fails() {
        for vacates in [false, true] {
            let mut state = GameState::new();
            place(&mut state, "A0", 2, 5);
            place(&mut state, "A1", 3, 5);
            let mut commands = vec![movement("A0", "RIGHT")];
            if vacates {
                commands.push(movement("A1", "RIGHT"));
            }
            state.step(&commands, &[]).unwrap();
            assert_eq!(robot(&state, "A0").x, 2);
            assert_eq!(robot(&state, "A1").x, if vacates { 4 } else { 3 });
        }
    }

    #[test]
    fn swapping_positions_fails_for_friends_and_opponents() {
        for second in ["A1", "B0"] {
            let mut state = GameState::new();
            place(&mut state, "A0", 6, 5);
            place(&mut state, second, 7, 5);
            let mut a = vec![movement("A0", "RIGHT")];
            let mut b = Vec::new();
            if second == "A1" {
                a.push(movement(second, "LEFT"));
            } else {
                b.push(movement(second, "LEFT"));
            }
            let before = state.robots.clone();
            state.step(&a, &b).unwrap();
            assert_eq!(state.robots, before);
        }
    }

    #[test]
    fn chain_following_moves_fail_except_front_robot() {
        let mut state = GameState::new();
        for (id, x) in [("A0", 2), ("A1", 3), ("A2", 4)] {
            place(&mut state, id, x, 5);
        }
        state
            .step(
                &[
                    movement("A0", "RIGHT"),
                    movement("A1", "RIGHT"),
                    movement("A2", "RIGHT"),
                ],
                &[],
            )
            .unwrap();
        assert_eq!(robot(&state, "A0").x, 2);
        assert_eq!(robot(&state, "A1").x, 3);
        assert_eq!(robot(&state, "A2").x, 5);
    }

    #[test]
    fn two_or_more_valid_moves_to_same_empty_cell_all_fail() {
        for opponents in [false, true] {
            let mut state = GameState::new();
            place(&mut state, "A0", 6, 5);
            let second = if opponents { "B0" } else { "A1" };
            place(&mut state, second, 8, 5);
            cargo(&mut state, "A0", 4);
            let mut a = vec![movement("A0", "RIGHT")];
            let mut b = Vec::new();
            if opponents {
                b.push(movement(second, "LEFT"));
            } else {
                a.push(movement(second, "LEFT"));
            }
            let before = state.robots.clone();
            state.step(&a, &b).unwrap();
            assert_eq!(state.robots, before);
        }
        let mut state = GameState::new();
        for (id, x, y) in [("A0", 6, 5), ("A1", 7, 4), ("A2", 7, 6), ("B0", 8, 5)] {
            place(&mut state, id, x, y);
        }
        let before = state.robots.clone();
        state
            .step(
                &[
                    movement("A0", "RIGHT"),
                    movement("A1", "DOWN"),
                    movement("A2", "UP"),
                ],
                &[movement("B0", "LEFT")],
            )
            .unwrap();
        assert_eq!(state.robots, before);
    }

    #[test]
    fn pick_takes_minimum_of_room_and_resource_and_never_scores_directly() {
        for (available, initial_cargo, expected_cargo, expected_left) in [
            (4, 0, 4, 0),
            (2, 0, 2, 0),
            (4, 3, 4, 3),
            (2, 3, 4, 1),
            (4, 4, 4, 4),
            (0, 0, 0, 0),
        ] {
            let mut state = depleted();
            let resource = &mut state.resources[0];
            resource.amount = available;
            let (x, y) = (resource.x, resource.y);
            place(&mut state, "A0", x, y);
            robot_mut(&mut state, "A0").cargo = initial_cargo;
            state.scores.a -= u16::from(available + initial_cargo);
            let score_before = state.scores;
            state.step(&[pick("A0")], &[]).unwrap();
            assert_eq!(robot(&state, "A0").cargo, expected_cargo);
            assert_eq!(state.resources[0].amount, expected_left);
            assert_eq!(state.scores, score_before);
            assert_eq!(state.resources.len(), 24);
        }
        let mut state = GameState::new();
        place(&mut state, "A0", 1, 3);
        state.step(&[pick("A0")], &[]).unwrap();
        assert_eq!(robot(&state, "A0").cargo, 0);
    }

    #[test]
    fn failed_move_on_resource_does_not_fall_back_to_pick() {
        let mut state = GameState::new();
        place(&mut state, "A0", 3, 2);
        place(&mut state, "A1", 4, 2);
        let before = state.resources.clone();
        state.step(&[movement("A0", "RIGHT")], &[]).unwrap();
        assert_eq!(state.resources, before);
        assert_eq!(robot(&state, "A0").cargo, 0);
    }

    #[test]
    fn arriving_in_depot_or_waiting_there_automatically_deposits_all_cargo() {
        let mut state = GameState::new();
        place(&mut state, "A0", 1, 3);
        place(&mut state, "B0", 13, 3);
        cargo(&mut state, "A0", 4);
        cargo(&mut state, "B0", 3);
        cargo(&mut state, "A1", 2);
        state
            .step(&[movement("A0", "LEFT")], &[movement("B0", "RIGHT")])
            .unwrap();
        assert_eq!(state.scores, Scores { a: 6, b: 3 });
        assert_eq!(robot(&state, "A0").x, 0);
        assert_eq!(robot(&state, "B0").x, 14);
        assert!(state.robots.iter().all(|r| r.cargo == 0));
    }

    #[test]
    fn missing_impossible_and_duplicate_commands_wait_but_valid_commands_survive() {
        let mut state = GameState::new();
        place(&mut state, "A0", 3, 2);
        let before = robot(&state, "A0").clone();
        state
            .step(
                &[
                    pick("A0"),
                    command("A0", "UNKNOWN", None),
                    movement("A1", "RIGHT"),
                    movement("B0", "LEFT"),
                    movement("does-not-exist", "RIGHT"),
                ],
                &[],
            )
            .unwrap();
        assert_eq!(robot(&state, "A0"), &before);
        assert_eq!(robot(&state, "A1").x, 1);
        assert_eq!(robot(&state, "A2").x, 0);
        assert_eq!(robot(&state, "B0").x, 14);
        let mut state = GameState::new();
        state
            .step(
                &[
                    command("A0", "TELEPORT", None),
                    command("A1", "MOVE", Some("DIAGONAL")),
                    command("A2", "MOVE", None),
                ],
                &[command("B0", "PICK", Some("DIAGONAL"))],
            )
            .unwrap();
        assert_eq!(state.robots, GameState::new().robots);
    }

    #[test]
    fn duplicate_two_valid_moves_and_duplicate_pick_and_move_cannot_act() {
        for commands in [
            vec![movement("A0", "RIGHT"), movement("A0", "DOWN")],
            vec![pick("A0"), movement("A0", "RIGHT")],
        ] {
            let mut state = GameState::new();
            place(&mut state, "A0", 3, 2);
            let before = state.clone();
            state.step(&commands, &[]).unwrap();
            assert_eq!(state.robots, before.robots);
            assert_eq!(state.resources, before.resources);
        }
    }

    #[test]
    fn all_delivered_ends_after_deposit_with_correct_winner_or_draw() {
        for (a, b, winner) in [
            (92, 0, Some(Team::A)),
            (0, 92, Some(Team::B)),
            (44, 48, None),
        ] {
            let mut state = depleted();
            state.scores = Scores { a, b };
            robot_mut(&mut state, "A0").cargo = 4;
            state.step(&[], &[]).unwrap();
            assert_eq!(state.completed_turn, 1);
            assert_eq!(
                state.outcome,
                Some(Outcome {
                    winner,
                    reason: EndReason::AllDelivered
                })
            );
            state.validate().unwrap();
            let final_state = state.clone();
            assert!(state.step(&[], &[]).is_err());
            assert_eq!(state, final_state);
        }
    }

    #[test]
    fn empty_map_with_carried_resources_does_not_finish_early() {
        let mut state = depleted();
        state.scores.a -= 4;
        place(&mut state, "A0", 2, 3);
        robot_mut(&mut state, "A0").cargo = 4;
        state.step(&[], &[]).unwrap();
        assert!(state.resources.iter().all(|r| r.amount == 0));
        assert!(state.outcome.is_none());
        assert_eq!(state.completed_turn, 1);
        state.step(&[movement("A0", "LEFT")], &[]).unwrap();
        assert!(state.outcome.is_none());
        state.step(&[movement("A0", "LEFT")], &[]).unwrap();
        assert_eq!(state.outcome.unwrap().reason, EndReason::AllDelivered);
        assert_eq!(state.scores.a, 96);
    }

    #[test]
    fn turn_200_applies_moves_and_deposits_before_result_then_rejects_turn_201() {
        let mut state = depleted();
        state.scores = Scores { a: 44, b: 44 };
        state.completed_turn = 199;
        place(&mut state, "A0", 1, 3);
        place(&mut state, "B0", 2, 3);
        robot_mut(&mut state, "A0").cargo = 4;
        robot_mut(&mut state, "B0").cargo = 4;
        state.step(&[movement("A0", "LEFT")], &[]).unwrap();
        assert_eq!(state.completed_turn, 200);
        assert_eq!(state.scores, Scores { a: 48, b: 44 });
        assert_eq!(robot(&state, "A0").cargo, 0);
        assert_eq!(robot(&state, "B0").cargo, 4);
        assert_eq!(
            state.outcome,
            Some(Outcome {
                winner: Some(Team::A),
                reason: EndReason::TurnLimit
            })
        );
        let final_state = state.clone();
        assert!(state.step(&[], &[]).is_err());
        assert_eq!(state, final_state);
    }

    #[test]
    fn unreturned_cargo_does_not_break_tie_at_turn_limit() {
        let mut state = GameState::new();
        state.completed_turn = 199;
        place(&mut state, "A0", 1, 3);
        cargo(&mut state, "A0", 4);
        state.step(&[], &[]).unwrap();
        assert_eq!(state.scores, Scores::default());
        assert_eq!(
            state.outcome,
            Some(Outcome {
                winner: None,
                reason: EndReason::TurnLimit
            })
        );
        assert_eq!(robot(&state, "A0").cargo, 4);
    }

    #[test]
    fn order_of_robot_resource_and_command_arrays_never_changes_rulings() {
        let mut state = GameState::new();
        place(&mut state, "A0", 6, 5);
        place(&mut state, "B0", 8, 5);
        place(&mut state, "A1", 3, 7);
        place(&mut state, "B1", 11, 7);
        let a = vec![movement("A0", "RIGHT"), pick("A1"), movement("A2", "RIGHT")];
        let b = vec![movement("B0", "LEFT"), pick("B1"), movement("B2", "LEFT")];
        let mut reverse = state.clone();
        reverse.robots.reverse();
        reverse.resources.reverse();
        let mut reversed_a = a.clone();
        let mut reversed_b = b.clone();
        reversed_a.reverse();
        reversed_b.reverse();
        state.step(&a, &b).unwrap();
        reverse.step(&reversed_a, &reversed_b).unwrap();
        assert_eq!(canonical(state), canonical(reverse));
    }

    #[test]
    fn exchanging_teams_by_mirroring_gives_mirrored_decisions() {
        let mut state = GameState::new();
        place(&mut state, "A0", 6, 5);
        place(&mut state, "B0", 8, 5);
        place(&mut state, "A1", 3, 7);
        place(&mut state, "B1", 11, 7);
        let mut mirrored = state.clone();
        for robot in &mut mirrored.robots {
            robot.x = 14 - robot.x;
            robot.team = if robot.team == Team::A {
                Team::B
            } else {
                Team::A
            };
            robot
                .id
                .replace_range(..1, if robot.team == Team::A { "A" } else { "B" });
        }
        let a = vec![movement("A0", "RIGHT"), pick("A1"), movement("A2", "RIGHT")];
        let b = vec![movement("B0", "LEFT"), pick("B1")];
        state.step(&a, &b).unwrap();
        let mirror_commands = |commands: &[Command]| {
            commands
                .iter()
                .map(|c| {
                    let mut c = c.clone();
                    c.robot_id.replace_range(
                        ..1,
                        if c.robot_id.starts_with('A') {
                            "B"
                        } else {
                            "A"
                        },
                    );
                    c.direction = c.direction.map(|direction| match direction.as_str() {
                        "RIGHT" => "LEFT".to_owned(),
                        "LEFT" => "RIGHT".to_owned(),
                        _ => direction,
                    });
                    c
                })
                .collect::<Vec<_>>()
        };
        mirrored
            .step(&mirror_commands(&b), &mirror_commands(&a))
            .unwrap();
        for robot in &mut mirrored.robots {
            robot.x = 14 - robot.x;
            robot.team = if robot.team == Team::A {
                Team::B
            } else {
                Team::A
            };
            robot
                .id
                .replace_range(..1, if robot.team == Team::A { "A" } else { "B" });
        }
        std::mem::swap(&mut mirrored.scores.a, &mut mirrored.scores.b);
        assert_eq!(canonical(state), canonical(mirrored));
    }

    #[test]
    fn deterministic_long_match_preserves_all_invariants_every_turn() {
        let mut first = GameState::new();
        let mut second = first.clone();
        let directions = ["UP", "RIGHT", "DOWN", "LEFT"];
        for turn in 0..MAX_TURNS {
            let mut a = Vec::new();
            let mut b = Vec::new();
            for (index, robot) in first.robots.iter().enumerate() {
                let action = match (usize::from(turn) + index) % 7 {
                    0 => pick(&robot.id),
                    1 => command(&robot.id, "INVALID", None),
                    _ => movement(&robot.id, directions[(usize::from(turn) / 3 + index) % 4]),
                };
                if robot.team == Team::A {
                    a.push(action);
                } else {
                    b.push(action);
                }
            }
            first.step(&a, &b).unwrap();
            second.step(&a, &b).unwrap();
            first.validate().unwrap();
            assert_eq!(first, second);
            if first.outcome.is_some() {
                break;
            }
        }
        assert!(first.outcome.is_some());
        assert!(first.completed_turn <= MAX_TURNS);
    }

    #[test]
    fn malformed_state_is_rejected_before_any_mutation() {
        let mut invalid_states = Vec::new();
        let mut state = GameState::new();
        state.resources[0].amount = 3;
        invalid_states.push(state);
        let mut state = GameState::new();
        state.robots[0].cargo = 5;
        invalid_states.push(state);
        let mut state = GameState::new();
        state.robots[0].x = 14;
        invalid_states.push(state);
        let mut state = GameState::new();
        state.robots[0].x = state.robots[1].x;
        state.robots[0].y = state.robots[1].y;
        invalid_states.push(state);
        let mut state = GameState::new();
        state.obstacles.push(Position { x: 0, y: 3 });
        invalid_states.push(state);
        for mut state in invalid_states {
            let before = state.clone();
            assert!(state.validate().is_err());
            assert!(state.step(&[movement("A0", "RIGHT")], &[]).is_err());
            assert_eq!(state, before);
        }
    }
}

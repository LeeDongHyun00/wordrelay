use crate::dictionary::{Dictionary, Word, allowed_starts};
use serde::Serialize;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const RECONNECT_GRACE: Duration = Duration::from_secs(6);
pub const ROUND_BREAK: Duration = Duration::from_secs(4);
pub fn answer_points(length: usize, remaining_ms: u64, budget_ms: u64) -> u32 {
    100 + 20 * length.saturating_sub(1) as u32
        + (200 * remaining_ms.min(budget_ms) / budget_ms.max(1)) as u32
}
// Display seconds slow down below 2s, with a further slowdown below 1s.
// The server deadline uses real time; clients invert this curve for display.
pub fn real_turn_budget(display_ms: u64) -> u64 {
    if display_ms <= 1000 {
        display_ms * 4
    } else if display_ms <= 2000 {
        4000 + (display_ms - 1000) * 2
    } else {
        6000 + display_ms - 2000
    }
}
pub const COUNTDOWN: Duration = Duration::from_secs(3);
pub fn turn_budget(accepted: u32) -> u64 {
    6000_u64.saturating_sub(u64::from(accepted) * 100).max(1000)
}
#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Lobby,
    Playing,
    Intermission,
    Finished,
}
pub struct Player {
    pub id: String,
    pub token: String,
    pub name: String,
    pub ready: bool,
    pub alive: bool,
    pub connected: bool,
    pub left: bool,
    pub disconnected_at: Option<Instant>,
    pub score: u32,
    pub round_score: u32,
    pub round_penalty: u32,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub player_id: Option<String>,
    pub name: String,
    pub word: Word,
    pub points: u32,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundResult {
    pub round: u32,
    pub winner_id: Option<String>,
    pub winner_ids: Vec<String>,
    pub failed_id: Option<String>,
    pub scores: Vec<RoundScore>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundScore {
    pub player_id: String,
    pub points: u32,
    pub total: u32,
    pub penalty: u32,
}
pub struct Game {
    pub code: String,
    pub players: Vec<Player>,
    pub host_id: String,
    pub phase: Phase,
    pub turn: usize,
    pub turn_id: u64,
    pub accepted: u32,
    pub starts_at: Option<Instant>,
    pub deadline: Option<Instant>,
    pub previous: Option<usize>,
    pub used: HashSet<usize>,
    pub history: Vec<History>,
    pub winner_id: Option<String>,
    pub notice: String,
    pub updated: Instant,
    pub total_rounds: u32,
    pub round: u32,
    pub next_round_at: Option<Instant>,
    pub round_results: Vec<RoundResult>,
    pub winner_ids: Vec<String>,
}
#[derive(Debug)]
pub struct GameError {
    pub code: &'static str,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, GameError>;
pub fn err(code: &'static str, message: impl Into<String>) -> GameError {
    GameError {
        code,
        message: message.into(),
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerView {
    id: String,
    name: String,
    ready: bool,
    alive: bool,
    connected: bool,
    left: bool,
    score: u32,
    round_score: u32,
    round_penalty: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub code: String,
    pub phase: Phase,
    pub host_id: String,
    pub players: Vec<PlayerView>,
    pub turn_player_id: Option<String>,
    pub turn_id: u64,
    pub accepted_count: u32,
    pub turn_duration_ms: u64,
    pub starts_at: Option<u64>,
    pub deadline: Option<u64>,
    pub server_now: u64,
    pub current_word: Option<Word>,
    pub history: Vec<History>,
    pub winner_id: Option<String>,
    pub notice: String,
    pub can_start: bool,
    pub dictionary_version: String,
    pub total_rounds: u32,
    pub round: u32,
    pub next_round_at: Option<u64>,
    pub round_results: Vec<RoundResult>,
    pub winner_ids: Vec<String>,
}
impl Game {
    pub fn new(code: String, now: Instant) -> Self {
        Self {
            code,
            players: vec![],
            host_id: String::new(),
            phase: Phase::Lobby,
            turn: 0,
            turn_id: 0,
            accepted: 0,
            starts_at: None,
            deadline: None,
            previous: None,
            used: HashSet::new(),
            history: vec![],
            winner_id: None,
            notice: String::new(),
            updated: now,
            total_rounds: 3,
            round: 0,
            next_round_at: None,
            round_results: vec![],
            winner_ids: vec![],
        }
    }
    pub fn attach(&mut self, name: &str, token: Option<&str>, now: Instant) -> Result<usize> {
        if let Some(token) = token {
            let Some(i) = self
                .players
                .iter()
                .position(|p| p.token == token && !p.left)
            else {
                return Err(err(
                    "SESSION_EXPIRED",
                    "입장 정보가 만료되었습니다. 다시 입장해 주세요.",
                ));
            };
            if self.players[i]
                .disconnected_at
                .is_some_and(|t| now.duration_since(t) >= RECONNECT_GRACE)
            {
                return Err(err(
                    "SESSION_EXPIRED",
                    "연결 복구 시간이 6초를 지나 퇴장했습니다.",
                ));
            }
            self.players[i].connected = true;
            self.players[i].disconnected_at = None;
            if self.phase != Phase::Playing {
                self.players[i].ready = false;
            }
            self.transfer_host();
            self.updated = now;
            return Ok(i);
        }
        if self.phase != Phase::Lobby {
            return Err(err("ALREADY_STARTED", "이미 게임이 시작된 방입니다."));
        }
        if self.players.len() >= 4 {
            return Err(err("ROOM_FULL", "이 방은 이미 4명이 함께하고 있어요."));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 12 || name.chars().any(char::is_control) {
            return Err(err("INVALID_NAME", "이름은 1~12글자로 입력해 주세요."));
        }
        let id = Uuid::new_v4().to_string();
        if self.players.is_empty() {
            self.host_id = id.clone()
        }
        self.players.push(Player {
            id,
            token: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            ready: false,
            alive: true,
            connected: true,
            left: false,
            disconnected_at: None,
            score: 0,
            round_score: 0,
            round_penalty: 0,
        });
        self.reset_ready();
        self.updated = now;
        Ok(self.players.len() - 1)
    }
    fn reset_ready(&mut self) {
        for p in &mut self.players {
            p.ready = false
        }
    }
    fn transfer_host(&mut self) {
        if !self
            .players
            .iter()
            .any(|p| p.id == self.host_id && p.connected && !p.left)
            && let Some(p) = self.players.iter().find(|p| p.connected && !p.left)
        {
            self.host_id = p.id.clone()
        }
    }
    pub fn kick(&mut self, id: &str, target: &str, d: &Dictionary, now: Instant) -> Result<()> {
        if self.host_id != id
            || !self
                .players
                .iter()
                .any(|p| p.id == id && p.connected && !p.left)
        {
            return Err(err("HOST_ONLY", "방장만 강퇴할 수 있습니다."));
        }
        if id == target {
            return Err(err("CANNOT_KICK_SELF", "자신은 강퇴할 수 없습니다."));
        }
        if !self.players.iter().any(|p| p.id == target && !p.left) {
            return Err(err("NOT_MEMBER", "방 참가자가 아닙니다."));
        }
        self.disconnect(target, true, d, now);
        Ok(())
    }
    pub fn ready(&mut self, id: &str, ready: bool, now: Instant) -> Result<()> {
        if self.phase != Phase::Lobby {
            return Err(err("NOT_LOBBY", "대기방에서만 준비할 수 있어요."));
        }
        let p = self
            .players
            .iter_mut()
            .find(|p| p.id == id && p.connected && !p.left)
            .ok_or_else(|| err("NOT_MEMBER", "방 참가자가 아닙니다."))?;
        p.ready = ready;
        self.updated = now;
        Ok(())
    }
    pub fn can_start(&self) -> bool {
        self.phase == Phase::Lobby
            && self.players.len() >= 2
            && self
                .players
                .iter()
                .all(|p| p.ready && p.connected && !p.left)
    }
    pub fn start(&mut self, id: &str, d: &Dictionary, now: Instant) -> Result<()> {
        if self.host_id != id {
            return Err(err("HOST_ONLY", "방장만 시작할 수 있어요."));
        }
        if !self.can_start() {
            return Err(err(
                "NOT_READY",
                "2명 이상, 모두 준비해야 시작할 수 있어요.",
            ));
        }
        self.round = 0;
        self.used.clear();
        self.round_results.clear();
        self.winner_ids.clear();
        for p in &mut self.players {
            p.score = 0;
        }
        self.turn = (Uuid::new_v4().as_u128() as usize) % self.players.len();
        self.start_round(d, now);
        Ok(())
    }
    pub fn configure(&mut self, id: &str, rounds: u32, now: Instant) -> Result<()> {
        if id != self.host_id {
            return Err(err("HOST_ONLY", "방장만 라운드 수를 설정할 수 있습니다."));
        }
        if self.phase != Phase::Lobby {
            return Err(err("NOT_LOBBY", "대기방에서만 설정할 수 있습니다."));
        }
        if !(1..=10).contains(&rounds) {
            return Err(err("INVALID_ROUNDS", "라운드는 1~10으로 설정해 주세요."));
        }
        if self.total_rounds != rounds {
            self.total_rounds = rounds;
            self.reset_ready();
        }
        self.updated = now;
        Ok(())
    }
    fn start_round(&mut self, d: &Dictionary, now: Instant) {
        self.round += 1;
        self.phase = Phase::Playing;
        self.accepted = 0;
        self.history.clear();
        self.winner_id = None;
        self.next_round_at = None;
        for p in &mut self.players {
            p.alive = !p.left;
            p.round_score = 0;
            p.round_penalty = 0;
        }
        if self.round > 1 {
            self.next_turn();
        }
        self.new_seed(d);
        self.notice = format!("{} 라운드", self.round);
        self.begin_turn(now + COUNTDOWN);
        self.updated = now;
    }
    pub fn advance_round(&mut self, d: &Dictionary, now: Instant) -> bool {
        if self.phase != Phase::Intermission || self.next_round_at.is_none_or(|t| now < t) {
            return false;
        }
        self.start_round(d, now);
        true
    }
    fn finish_match(&mut self) {
        let best = self
            .players
            .iter()
            .filter(|p| !p.left)
            .map(|p| p.score)
            .max();
        self.winner_ids = self
            .players
            .iter()
            .filter(|p| !p.left && Some(p.score) == best)
            .map(|p| p.id.clone())
            .collect();
        self.phase = Phase::Finished;
        self.next_round_at = None;
        self.deadline = None;
        self.starts_at = None;
    }
    fn begin_turn(&mut self, now: Instant) {
        self.turn_id += 1;
        self.starts_at = Some(now);
        self.deadline =
            Some(now + Duration::from_millis(real_turn_budget(turn_budget(self.accepted))));
    }
    fn new_seed(&mut self, d: &Dictionary) {
        if let Some(seed) = d.seed(&self.used) {
            self.previous = Some(seed);
            self.used.insert(seed);
            self.push_history(History {
                player_id: None,
                name: "시작 단어".into(),
                word: d.word(seed),
                points: 0,
            });
        } else {
            self.phase = Phase::Finished;
            self.deadline = None;
            self.notice = "이어갈 단어를 모두 사용했어요. 무승부!".into();
        }
    }
    fn push_history(&mut self, h: History) {
        self.history.push(h);
        if self.history.len() > 12 {
            self.history.remove(0);
        }
    }
    fn next_turn(&mut self) {
        for _ in 0..self.players.len() {
            self.turn = (self.turn + 1) % self.players.len();
            if self.players[self.turn].alive && !self.players[self.turn].left {
                break;
            }
        }
    }
    fn finish_round(&mut self, failed: usize, now: Instant) {
        let alive: Vec<_> = self.players.iter().filter(|p| p.alive && !p.left).collect();
        let winners: Vec<String> = alive.iter().map(|p| p.id.clone()).collect();
        self.winner_id = if winners.len() == 1 {
            winners.first().cloned()
        } else {
            None
        };
        let failed_id = Some(self.players[failed].id.clone());
        for p in self.players.iter_mut().filter(|p| winners.contains(&p.id)) {
            p.score += 300;
            p.round_score += 300;
        }
        self.round_results.push(RoundResult {
            round: self.round,
            winner_id: self.winner_id.clone(),
            winner_ids: winners,
            failed_id,
            scores: self
                .players
                .iter()
                .map(|p| RoundScore {
                    player_id: p.id.clone(),
                    points: p.round_score,
                    total: p.score,
                    penalty: p.round_penalty,
                })
                .collect(),
        });
        self.deadline = None;
        self.starts_at = None;
        self.turn_id += 1;
        if self.round < self.total_rounds && self.players.iter().filter(|p| !p.left).count() >= 2 {
            self.phase = Phase::Intermission;
            self.next_round_at = Some(now + ROUND_BREAK);
        } else {
            self.finish_match();
        }
    }
    pub fn submit(
        &mut self,
        id: &str,
        turn_id: u64,
        input: &str,
        d: &Dictionary,
        now: Instant,
    ) -> Result<()> {
        if self.phase != Phase::Playing {
            return Err(err("NOT_PLAYING", "진행 중인 게임이 아닙니다."));
        }
        if now >= self.deadline.unwrap() {
            self.timeout(d, now);
            return Err(err("TIME_UP", "제한 시간이 지났어요."));
        }
        if now < self.starts_at.unwrap() {
            return Err(err("COUNTDOWN", "시작 카운트다운 중이에요."));
        }
        if self.turn_id != turn_id {
            return Err(err("STALE_TURN", "이미 차례가 바뀌었어요."));
        }
        if self.players[self.turn].id != id {
            return Err(err("NOT_YOUR_TURN", "지금은 다른 참가자의 차례예요."));
        }
        if input.chars().count() > 256 {
            return Err(err("INVALID_WORD", "단어가 너무 길어요."));
        }
        let next = d.lookup(input).ok_or_else(|| {
            err(
                "NOT_FOUND",
                "통합 사전에 없는 단어예요. 다시 입력해 주세요.",
            )
        })?;
        if self.used.contains(&next) {
            return Err(err("ALREADY_USED", "이번 게임에서 이미 사용한 단어입니다."));
        }
        let prev = self.previous.unwrap();
        if !d.follows(prev, next) {
            return Err(err(
                "WRONG_START",
                format!(
                    "{}로 시작해야 해요.",
                    allowed_starts(&d.entries[prev].last_syllable).join(" 또는 ")
                ),
            ));
        }
        self.used.insert(next);
        self.previous = Some(next);
        let points = answer_points(
            d.entries[next].reading.chars().count(),
            self.deadline.unwrap().duration_since(now).as_millis() as u64,
            real_turn_budget(turn_budget(self.accepted)),
        );
        self.accepted += 1;
        self.players[self.turn].score += points;
        self.players[self.turn].round_score += points;
        self.push_history(History {
            player_id: Some(id.to_owned()),
            name: self.players[self.turn].name.clone(),
            word: d.word(next),
            points,
        });
        self.notice = String::new();
        if self.phase == Phase::Playing {
            self.next_turn();
            self.begin_turn(now);
        }
        self.updated = now;
        Ok(())
    }
    fn eliminate(&mut self, index: usize) {
        let p = &mut self.players[index];
        if !p.alive {
            return;
        }
        let penalty = (p.round_score / 5).min(150);
        p.round_penalty = penalty;
        p.round_score -= penalty;
        p.score -= penalty;
        p.alive = false;
    }
    pub fn timeout(&mut self, _d: &Dictionary, now: Instant) -> bool {
        if self.phase != Phase::Playing || self.deadline.is_none_or(|end| now < end) {
            return false;
        }
        self.eliminate(self.turn);
        let name = self.players[self.turn].name.clone();
        self.notice = format!("{name} 님 시간 초과 · 라운드 종료");
        self.finish_round(self.turn, now);
        self.updated = now;
        true
    }
    pub fn disconnect(&mut self, id: &str, leave: bool, _d: &Dictionary, now: Instant) {
        let Some(i) = self.players.iter().position(|p| p.id == id) else {
            return;
        };
        self.players[i].connected = false;
        self.players[i].disconnected_at = Some(now);
        self.players[i].ready = false;
        if leave {
            if self.phase == Phase::Playing {
                self.eliminate(i);
            }
            self.players[i].left = true;
            self.players[i].alive = false;
        }
        if self.phase == Phase::Lobby {
            if leave {
                self.players.remove(i);
            }
            self.reset_ready();
        } else if self.phase == Phase::Playing && leave {
            self.notice = format!("{} 님 퇴장 · 라운드 종료", self.players[i].name);
            self.finish_round(i, now);
        }
        if self.phase == Phase::Intermission && self.players.iter().filter(|p| !p.left).count() < 2
        {
            self.finish_match();
        }
        if self.phase == Phase::Finished {
            self.finish_match();
        }
        self.transfer_host();
        self.updated = now;
    }
    pub fn sweep(&mut self, d: &Dictionary, now: Instant) -> bool {
        let expired: Vec<_> = self
            .players
            .iter()
            .filter(|p| {
                !p.connected
                    && !p.left
                    && p.disconnected_at
                        .is_some_and(|t| now.duration_since(t) >= RECONNECT_GRACE)
            })
            .map(|p| p.id.clone())
            .collect();
        for id in &expired {
            self.disconnect(id, true, d, now);
        }
        !expired.is_empty()
    }
    pub fn rematch(&mut self, id: &str, now: Instant) -> Result<()> {
        if self.host_id != id {
            return Err(err("HOST_ONLY", "방장만 대기방으로 돌아갈 수 있어요."));
        }
        if self.phase != Phase::Finished {
            return Err(err("NOT_FINISHED", "게임이 끝난 뒤 다시 준비할 수 있어요."));
        }
        self.players.retain(|p| p.connected && !p.left);
        self.phase = Phase::Lobby;
        self.reset_ready();
        for p in &mut self.players {
            p.alive = true;
            p.score = 0;
            p.round_score = 0;
            p.round_penalty = 0;
        }
        self.round = 0;
        self.round_results.clear();
        self.winner_ids.clear();
        self.next_round_at = None;
        self.previous = None;
        self.history.clear();
        self.used.clear();
        self.winner_id = None;
        self.accepted = 0;
        self.notice = String::new();
        self.transfer_host();
        self.updated = now;
        Ok(())
    }
    pub fn snapshot(&self, d: &Dictionary, now: Instant, unix_ms: u64) -> Snapshot {
        let timestamp = |time: Instant| {
            if time >= now {
                unix_ms + time.duration_since(now).as_millis() as u64
            } else {
                unix_ms.saturating_sub(now.duration_since(time).as_millis() as u64)
            }
        };
        Snapshot {
            code: self.code.clone(),
            phase: self.phase,
            host_id: self.host_id.clone(),
            players: self
                .players
                .iter()
                .map(|p| PlayerView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    ready: p.ready,
                    alive: p.alive,
                    connected: p.connected,
                    left: p.left,
                    score: p.score,
                    round_score: p.round_score,
                    round_penalty: p.round_penalty,
                })
                .collect(),
            turn_player_id: if self.phase == Phase::Playing {
                Some(self.players[self.turn].id.clone())
            } else {
                None
            },
            turn_id: self.turn_id,
            accepted_count: self.accepted,
            turn_duration_ms: turn_budget(self.accepted),
            starts_at: self.starts_at.map(timestamp),
            deadline: self.deadline.map(timestamp),
            server_now: unix_ms,
            current_word: self.previous.map(|i| d.word(i)),
            history: self.history.clone(),
            winner_id: self.winner_id.clone(),
            notice: self.notice.clone(),
            can_start: self.can_start(),
            dictionary_version: d.version.clone(),
            total_rounds: self.total_rounds,
            round: self.round,
            next_round_at: self.next_round_at.map(timestamp),
            round_results: self.round_results.clone(),
            winner_ids: self.winner_ids.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    fn dict() -> &'static Dictionary {
        static DICT: OnceLock<Dictionary> = OnceLock::new();
        DICT.get_or_init(|| Dictionary::load("data/dictionary.json").unwrap())
    }
    fn lobby(n: usize) -> (Game, Instant) {
        let now = Instant::now();
        let mut g = Game::new("ABC234".into(), now);
        for i in 0..n {
            g.attach(&format!("사람{i}"), None, now).unwrap();
        }
        (g, now)
    }
    fn started(n: usize) -> (Game, Instant) {
        let (mut g, now) = lobby(n);
        g.total_rounds = 1;
        for p in &mut g.players {
            p.ready = true;
        }
        g.start(&g.host_id.clone(), dict(), now).unwrap();
        g.turn = 0;
        (g, now + COUNTDOWN)
    }
    #[test]
    fn capped_penalty_and_survival_bonus_allow_a_comeback() {
        let (mut g, now) = started(2);
        g.players[0].score = 500;
        g.players[0].round_score = 500;
        g.players[1].score = 150;
        g.players[1].round_score = 150;
        let challenger = g.players[1].id.clone();
        g.timeout(dict(), now + Duration::from_secs(10));
        assert_eq!(g.players[0].score, 400);
        assert_eq!(g.players[1].score, 450);
        assert_eq!(g.winner_ids, vec![challenger]);
    }
    #[test]
    fn elimination_penalty_preserves_prior_rounds_caps_and_applies_once() {
        let (mut g, now) = started(3);
        g.players[0].score = 1200;
        g.players[0].round_score = 200;
        g.eliminate(0);
        assert_eq!(g.players[0].score, 1160);
        assert_eq!(g.players[0].round_score, 160);
        assert_eq!(g.players[0].round_penalty, 40);
        g.disconnect(&g.players[0].id.clone(), true, dict(), now);
        assert_eq!(g.players[0].score, 1160);
        g.players[1].score = 2000;
        g.players[1].round_score = 1000;
        g.eliminate(1);
        assert_eq!(g.players[1].score, 1850);
        g.players[2].score = 0;
        g.players[2].round_score = 0;
        g.eliminate(2);
        assert_eq!(g.players[2].score, 0);
    }
    #[test]
    fn speed_length_scoring_is_bounded_by_actual_turn_budget() {
        assert_eq!(answer_points(4, 3000, 6000), 260);
        assert_eq!(answer_points(4, 500, 1000), 260);
        assert_eq!(answer_points(1, 6000, 6000), 300);
        assert_eq!(answer_points(5, 0, 6000), 180);
        assert!(answer_points(5, 2000, 6000) > answer_points(4, 2000, 6000));
    }
    #[test]
    fn rounds_keep_points_reset_timer_and_report_ties() {
        let (mut g, now) = started(2);
        g.total_rounds = 2;
        let first = g.players[0].id.clone();
        let second = g.players[1].id.clone();
        g.timeout(dict(), now + Duration::from_secs(10));
        assert_eq!(g.phase, Phase::Intermission);
        assert_eq!(g.players[1].score, 300);
        let next = g.next_round_at.unwrap();
        assert!(!g.advance_round(dict(), next - Duration::from_millis(1)));
        assert!(g.advance_round(dict(), next));
        assert_eq!(g.round, 2);
        assert!(g.players.iter().all(|p| p.alive && p.round_score == 0));
        assert_eq!(g.accepted, 0);
        assert_eq!(
            g.deadline.unwrap() - g.starts_at.unwrap(),
            Duration::from_secs(10)
        );
        g.turn = 1;
        g.timeout(dict(), g.deadline.unwrap());
        assert_eq!(g.phase, Phase::Finished);
        assert_eq!(g.round_results.len(), 2);
        assert_eq!(g.winner_ids, vec![first, second]);
        let points: u32 = g.players.iter().map(|p| p.score).sum();
        assert!(!g.timeout(dict(), now + Duration::from_secs(99)));
        assert_eq!(g.players.iter().map(|p| p.score).sum::<u32>(), points);
    }
    #[test]
    fn disconnect_expires_in_game_and_cannot_resume_at_boundary() {
        let (mut g, now) = started(3);
        let id = g.players[1].id.clone();
        let token = g.players[1].token.clone();
        g.disconnect(&id, false, dict(), now);
        assert!(!g.sweep(dict(), now + Duration::from_millis(5999)));
        assert!(
            g.attach("복귀", Some(&token), now + Duration::from_secs(6))
                .is_err()
        );
        assert!(g.sweep(dict(), now + Duration::from_secs(6)));
        assert!(g.players[1].left);
    }
    #[test]
    fn round_configuration_is_host_only_and_resets_ready() {
        let (mut g, now) = lobby(2);
        let host = g.host_id.clone();
        assert!(g.configure(&g.players[1].id.clone(), 2, now).is_err());
        for p in &mut g.players {
            p.ready = true;
        }
        assert!(g.configure(&host, 0, now).is_err());
        assert!(g.configure(&host, 11, now).is_err());
        g.configure(&host, 2, now).unwrap();
        assert!(!g.can_start());
        assert_eq!(g.total_rounds, 2);
    }
    #[test]
    fn capacity_readiness_and_host_permissions() {
        let (mut g, now) = lobby(4);
        assert_eq!(
            g.attach("다섯번째", None, now).unwrap_err().code,
            "ROOM_FULL"
        );
        assert_eq!(
            g.start(&g.host_id.clone(), dict(), now).unwrap_err().code,
            "NOT_READY"
        );
        for p in &mut g.players {
            p.ready = true;
        }
        assert_eq!(
            g.start(&g.players[1].id.clone(), dict(), now)
                .unwrap_err()
                .code,
            "HOST_ONLY"
        );
        assert!(g.start(&g.host_id.clone(), dict(), now).is_ok());
        assert_eq!(
            g.attach("새사람", None, now).unwrap_err().code,
            "ALREADY_STARTED"
        );
    }
    #[test]
    fn readiness_resets_when_membership_changes() {
        let (mut g, now) = lobby(2);
        for p in &mut g.players {
            p.ready = true;
        }
        g.attach("셋", None, now).unwrap();
        assert!(g.players.iter().all(|p| !p.ready));
        for p in &mut g.players {
            p.ready = true;
        }
        g.disconnect(&g.players[0].id.clone(), false, dict(), now);
        assert!(!g.can_start());
        assert_eq!(g.host_id, g.players[1].id);
        assert!(g.players.iter().all(|p| !p.ready));
    }
    #[test]
    fn slow_clock_deadlines_and_four_player_rounds() {
        assert_eq!(real_turn_budget(6000), 10000);
        assert_eq!(real_turn_budget(2000), 6000);
        assert_eq!(real_turn_budget(1500), 5000);
        assert_eq!(real_turn_budget(1000), 4000);
        assert_eq!(real_turn_budget(500), 2000);
        let (mut g, now) = started(4);
        g.total_rounds = 2;
        assert!(!g.timeout(dict(), now + Duration::from_millis(9999)));
        assert!(g.timeout(dict(), now + Duration::from_secs(10)));
        assert_eq!(g.phase, Phase::Intermission);
        assert_eq!(g.round_results.len(), 1);
        assert_eq!(g.round_results[0].winner_ids.len(), 3);
        assert_eq!(g.round_results[0].failed_id, Some(g.players[0].id.clone()));
        assert!(g.advance_round(dict(), g.next_round_at.unwrap()));
        assert!(g.players.iter().all(|p| p.alive));
        assert_eq!(g.round, 2);
    }
    #[test]
    fn duration_is_integer_and_floors_at_one_second() {
        assert_eq!(turn_budget(0), 6000);
        assert_eq!(turn_budget(1), 5900);
        assert_eq!(turn_budget(49), 1100);
        assert_eq!(turn_budget(50), 1000);
        assert_eq!(turn_budget(500), 1000);
        assert_eq!(turn_budget(u32::MAX), 1000);
    }
    #[test]
    fn success_advances_once_and_invalid_answers_do_not_change_deadline() {
        let (mut g, now) = started(4);
        let id = g.players[0].id.clone();
        let turn = g.turn_id;
        let deadline = g.deadline;
        g.previous = Some(dict().lookup("사과").unwrap());
        g.used = HashSet::from([g.previous.unwrap()]);
        assert_eq!(
            g.submit(&id, turn, "없는단어용용용", dict(), now)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        assert_eq!(
            g.submit(&id, turn, "바다", dict(), now).unwrap_err().code,
            "WRONG_START"
        );
        assert_eq!(g.deadline, deadline);
        assert_eq!(
            g.submit(&id, turn, "사과", dict(), now).unwrap_err().code,
            "ALREADY_USED"
        );
        assert_eq!(
            g.submit(&g.players[1].id.clone(), turn, "과자", dict(), now)
                .unwrap_err()
                .code,
            "NOT_YOUR_TURN"
        );
        g.submit(&id, turn, "과자", dict(), now).unwrap();
        assert_eq!(g.accepted, 1);
        assert_eq!(g.turn, 1);
        assert_eq!(g.deadline, Some(now + Duration::from_millis(9900)));
        assert_eq!(
            g.submit(&id, turn, "자전거", dict(), now).unwrap_err().code,
            "STALE_TURN"
        );
        assert_eq!(g.accepted, 1);
    }
    #[test]
    fn deadline_boundary_eliminates_and_cannot_be_bypassed() {
        let (mut g, now) = started(4);
        let id = g.players[0].id.clone();
        let turn = g.turn_id;
        assert!(!g.timeout(dict(), now + Duration::from_millis(9999)));
        assert_eq!(
            g.submit(
                &id,
                turn,
                "과자",
                dict(),
                now + Duration::from_millis(10000)
            )
            .unwrap_err()
            .code,
            "TIME_UP"
        );
        assert!(!g.players[0].alive);
        assert_eq!(g.phase, Phase::Finished);
        assert_eq!(g.round_results[0].winner_ids.len(), 3);
        assert_eq!(g.accepted, 0);
        assert!(!g.timeout(dict(), now + Duration::from_millis(10000))); // no double elimination
    }
    #[test]
    fn countdown_disallows_early_answers() {
        let (mut g, start) = started(2);
        let id = g.players[0].id.clone();
        let turn = g.turn_id;
        assert_eq!(
            g.submit(&id, turn, "과자", dict(), start - Duration::from_millis(1))
                .unwrap_err()
                .code,
            "COUNTDOWN"
        );
    }
    #[test]
    fn disconnect_and_resume_cannot_reset_clock() {
        let (mut g, now) = started(3);
        let id = g.players[0].id.clone();
        let token = g.players[0].token.clone();
        let deadline = g.deadline;
        g.disconnect(&id, false, dict(), now);
        assert!(!g.players[0].connected);
        assert_eq!(g.deadline, deadline);
        g.attach("변경불가", Some(&token), now + Duration::from_secs(2))
            .unwrap();
        assert!(g.players[0].connected);
        assert_eq!(g.deadline, deadline);
        assert_eq!(g.players[0].name, "사람0");
        assert_eq!(
            g.attach("사람", Some("fake"), now).unwrap_err().code,
            "SESSION_EXPIRED"
        );
    }
    #[test]
    fn disconnect_slots_expire_only_in_lobby() {
        let (mut g, now) = lobby(4);
        let id = g.players[0].id.clone();
        let token = g.players[0].token.clone();
        g.disconnect(&id, false, dict(), now);
        assert!(!g.sweep(dict(), now + Duration::from_millis(5999)));
        assert!(g.sweep(dict(), now + Duration::from_secs(6)));
        assert_eq!(g.players.len(), 3);
        assert_eq!(
            g.attach("사람", Some(&token), now).unwrap_err().code,
            "SESSION_EXPIRED"
        );
    }
    #[test]
    fn last_survivor_wins_and_rematch_resets_everything() {
        let (mut g, now) = started(2);
        let winner = g.players[1].id.clone();
        g.timeout(dict(), now + Duration::from_secs(10));
        assert_eq!(g.phase, Phase::Finished);
        assert_eq!(g.winner_id, Some(winner));
        assert!(g.deadline.is_none());
        g.rematch(&g.host_id.clone(), now + Duration::from_secs(7))
            .unwrap();
        assert_eq!(g.phase, Phase::Lobby);
        assert!(g.used.is_empty());
        assert!(g.previous.is_none());
        assert!(g.history.is_empty());
        assert_eq!(g.accepted, 0);
        assert!(g.players.iter().all(|p| !p.ready && p.alive));
    }
    #[test]
    fn leaving_eliminates_and_transfers_host() {
        let (mut g, now) = started(3);
        let id = g.players[0].id.clone();
        g.disconnect(&id, true, dict(), now);
        assert!(!g.players[0].alive);
        assert!(g.players[0].left);
        assert_eq!(g.phase, Phase::Finished);
        assert_eq!(g.host_id, g.players[1].id);
        assert_eq!(
            g.attach("돌아오기", Some(&g.players[0].token.clone()), now)
                .unwrap_err()
                .code,
            "SESSION_EXPIRED"
        );
    }
    #[test]
    fn dead_end_answer_stays_visible_and_next_player_gets_their_turn() {
        let (mut g, now) = started(3);
        let d = dict();
        let seed = d.lookup("사과").unwrap();
        let next = d.lookup("과자").unwrap();
        g.previous = Some(seed);
        g.used = (0..d.entries.len())
            .filter(|i| d.entries[*i].first_syllable.as_ref() == "자")
            .collect();
        g.used.insert(seed);
        g.submit(&g.players[0].id.clone(), g.turn_id, "과자", d, now)
            .unwrap();
        assert_eq!(g.accepted, 1);
        assert_eq!(g.previous, Some(next));
        assert!(g.notice.is_empty());
        assert_eq!(g.turn, 1);
        assert_eq!(g.history.last().unwrap().word.label, "과자");
        assert!(g.history.last().unwrap().points > 0);
        assert_eq!(g.phase, Phase::Playing);
        assert!(g.deadline.unwrap() > now);
        assert!(g.players.iter().all(|p| p.alive));
        g.timeout(d, g.deadline.unwrap());
        assert_eq!(g.round_results[0].failed_id, Some(g.players[1].id.clone()));
    }
    #[test]
    fn words_are_reserved_across_rounds_but_reset_for_a_new_game() {
        let (mut g, now) = started(2);
        let d = dict();
        g.total_rounds = 2;
        let seed = d.lookup("나비").unwrap();
        let answer = d.lookup("비비").unwrap();
        g.previous = Some(seed);
        g.used.insert(seed);
        g.submit(&g.players[0].id.clone(), g.turn_id, "비비", d, now)
            .unwrap();
        g.timeout(d, g.deadline.unwrap());
        g.advance_round(d, g.next_round_at.unwrap());
        assert!(g.used.contains(&seed));
        assert!(g.used.contains(&answer));
        assert_ne!(g.previous, Some(seed));
        assert_ne!(g.previous, Some(answer));
        let start = g.starts_at.unwrap();
        let error = g
            .submit(&g.players[g.turn].id.clone(), g.turn_id, "비비", d, start)
            .unwrap_err();
        assert_eq!(error.code, "ALREADY_USED");
        g.timeout(d, g.deadline.unwrap());
        g.rematch(&g.host_id.clone(), start).unwrap();
        assert!(g.used.is_empty());
    }
    #[test]
    fn kicking_current_player_ends_round_once() {
        let (mut g, now) = started(3);
        let host = g.host_id.clone();
        g.turn = 1;
        let target = g.players[1].id.clone();
        let token = g.players[1].token.clone();
        let old_turn = g.turn_id;
        g.kick(&host, &target, dict(), now).unwrap();
        assert!(g.players[1].left);
        assert_eq!(g.phase, Phase::Finished);
        assert_eq!(g.round_results[0].winner_ids.len(), 2);
        assert!(g.turn_id > old_turn);
        assert!(g.attach("복귀", Some(&token), now).is_err());
        let last = g.players[2].id.clone();
        g.kick(&host, &last, dict(), now).unwrap();
        assert_eq!(g.phase, Phase::Finished);
        assert_eq!(g.winner_ids, vec![host]);
        assert_eq!(g.round_results.len(), 1);
    }
    #[test]
    fn snapshots_never_expose_session_tokens() {
        let (g, now) = lobby(2);
        let raw = serde_json::to_string(&g.snapshot(dict(), now, 10000)).unwrap();
        for p in &g.players {
            assert!(!raw.contains(&p.token));
        }
        assert!(!raw.contains("token"));
    }
}

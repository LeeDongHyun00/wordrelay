use crate::dictionary::{Dictionary, Word, allowed_starts};
use serde::Serialize;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const RECONNECT_GRACE: Duration = Duration::from_secs(30);
pub const COUNTDOWN: Duration = Duration::from_secs(3);
pub fn turn_budget(accepted: u32) -> u64 {
    6000_u64.saturating_sub(u64::from(accepted) * 100).max(1000)
}
#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Lobby,
    Playing,
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
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub player_id: Option<String>,
    pub name: String,
    pub word: Word,
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
        self.phase = Phase::Playing;
        self.accepted = 0;
        self.used.clear();
        self.history.clear();
        self.winner_id = None;
        for p in &mut self.players {
            p.alive = true;
            p.score = 0;
        }
        self.turn = (Uuid::new_v4().as_u128() as usize) % self.players.len();
        self.new_seed(d);
        self.notice = "잠시 후 시작! 마지막 글자를 이어 주세요.".to_owned();
        self.begin_turn(now + COUNTDOWN);
        self.updated = now;
        Ok(())
    }
    fn begin_turn(&mut self, now: Instant) {
        self.turn_id += 1;
        self.starts_at = Some(now);
        self.deadline = Some(now + Duration::from_millis(turn_budget(self.accepted)));
    }
    fn new_seed(&mut self, d: &Dictionary) {
        if let Some(seed) = d.seed(&self.used) {
            self.previous = Some(seed);
            self.used.insert(seed);
            self.push_history(History {
                player_id: None,
                name: "시작 단어".into(),
                word: d.word(seed),
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
    fn finish_if_needed(&mut self) -> bool {
        let alive: Vec<_> = self.players.iter().filter(|p| p.alive && !p.left).collect();
        if alive.len() > 1 {
            return false;
        }
        self.winner_id = alive.first().map(|p| p.id.clone());
        self.notice = alive
            .first()
            .map(|p| format!("{} 님이 마지막까지 살아남았어요!", p.name))
            .unwrap_or("남은 참가자가 없어 게임을 마쳤어요.".into());
        self.phase = Phase::Finished;
        self.deadline = None;
        self.starts_at = None;
        self.turn_id += 1;
        true
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
            return Err(err(
                "ALREADY_USED",
                "이미 나온 단어예요. 다른 단어를 입력해 주세요.",
            ));
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
        self.accepted += 1;
        self.players[self.turn].score += 1;
        self.push_history(History {
            player_id: Some(id.to_owned()),
            name: self.players[self.turn].name.clone(),
            word: d.word(next),
        });
        self.notice = String::new();
        if !d.has_next(next, &self.used) {
            self.new_seed(d);
            self.notice = "이어갈 단어가 없어 새 단어로 이어갑니다. 탈락 없이 계속!".into();
        }
        if self.phase == Phase::Playing {
            self.next_turn();
            self.begin_turn(now);
        }
        self.updated = now;
        Ok(())
    }
    pub fn timeout(&mut self, d: &Dictionary, now: Instant) -> bool {
        if self.phase != Phase::Playing || self.deadline.is_none_or(|end| now < end) {
            return false;
        }
        self.players[self.turn].alive = false;
        let name = self.players[self.turn].name.clone();
        if !self.finish_if_needed() {
            self.next_turn();
            self.new_seed(d);
            self.notice = format!("{name} 님 시간 초과! 새 단어로 계속합니다.");
            if self.phase == Phase::Playing {
                self.begin_turn(now)
            }
        }
        self.updated = now;
        true
    }
    pub fn disconnect(&mut self, id: &str, leave: bool, d: &Dictionary, now: Instant) {
        let Some(i) = self.players.iter().position(|p| p.id == id) else {
            return;
        };
        self.players[i].connected = false;
        self.players[i].disconnected_at = Some(now);
        self.players[i].ready = false;
        if leave {
            self.players[i].left = true;
            self.players[i].alive = false;
        }
        if self.phase == Phase::Lobby {
            if leave {
                self.players.remove(i);
            }
            self.reset_ready();
        } else if self.phase == Phase::Playing
            && leave
            && !self.finish_if_needed()
            && self.turn == i
        {
            self.next_turn();
            self.new_seed(d);
            if self.phase == Phase::Playing {
                self.begin_turn(now)
            }
        }
        self.transfer_host();
        self.updated = now;
    }
    pub fn sweep(&mut self, now: Instant) -> bool {
        if self.phase != Phase::Lobby {
            return false;
        }
        let before = self.players.len();
        self.players.retain(|p| {
            p.connected
                || p.disconnected_at
                    .is_none_or(|t| now.duration_since(t) < RECONNECT_GRACE)
        });
        if before != self.players.len() {
            self.reset_ready();
            self.transfer_host();
            return true;
        }
        false
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
        }
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
        for p in &mut g.players {
            p.ready = true;
        }
        g.start(&g.host_id.clone(), dict(), now).unwrap();
        g.turn = 0;
        (g, now + COUNTDOWN)
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
        assert_eq!(g.deadline, Some(now + Duration::from_millis(5900)));
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
        assert!(!g.timeout(dict(), now + Duration::from_millis(5999)));
        assert_eq!(
            g.submit(&id, turn, "과자", dict(), now + Duration::from_millis(6000))
                .unwrap_err()
                .code,
            "TIME_UP"
        );
        assert!(!g.players[0].alive);
        assert_eq!(g.turn, 1);
        assert_eq!(g.accepted, 0);
        assert!(!g.timeout(dict(), now + Duration::from_millis(6000))); // no double elimination
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
        assert!(!g.sweep(now + Duration::from_secs(29)));
        assert!(g.sweep(now + Duration::from_secs(30)));
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
        g.timeout(dict(), now + Duration::from_secs(6));
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
        assert_eq!(g.turn, 1);
        assert_eq!(g.host_id, g.players[1].id);
        assert_eq!(
            g.attach("돌아오기", Some(&g.players[0].token.clone()), now)
                .unwrap_err()
                .code,
            "SESSION_EXPIRED"
        );
    }
    #[test]
    fn word_with_no_continuation_starts_a_new_chain_without_elimination() {
        let (mut g, now) = started(3);
        let d = dict();
        let seed = d.lookup("사과").unwrap();
        let next = d.lookup("과자").unwrap();
        g.previous = Some(seed);
        g.used = (0..d.entries.len())
            .filter(|i| d.entries[*i].first_syllable == "자")
            .collect();
        g.used.insert(seed);
        g.submit(&g.players[0].id.clone(), g.turn_id, "과자", d, now)
            .unwrap();
        assert_eq!(g.accepted, 1);
        assert_ne!(g.previous, Some(next));
        assert!(g.notice.contains("새 단어"));
        assert!(g.players.iter().all(|p| p.alive));
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

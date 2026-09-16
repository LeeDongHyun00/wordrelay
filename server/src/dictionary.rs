use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Dataset {
    version: String,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub label: String,
    pub reading: String,
    aliases: Vec<String>,
    pub first_syllable: String,
    pub last_syllable: String,
    senses: Vec<Sense>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sense {
    category: String,
    definition: String,
    acceptance_reason: String,
    source: Source,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Source {
    name: String,
    url: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meaning {
    category: String,
    definition: String,
    reason: String,
    source: Source,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub label: String,
    pub reading: String,
    pub next_starts: Vec<String>,
    pub meanings: Vec<Meaning>,
}
pub struct Dictionary {
    pub version: String,
    pub entries: Vec<Entry>,
    keys: HashMap<String, usize>,
    first: HashMap<String, Vec<usize>>,
    seeds: Vec<usize>,
}
pub fn normalize(input: &str) -> String {
    input
        .nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace() && !c.is_punctuation())
        .collect()
}
pub fn allowed_starts(last: &str) -> Vec<String> {
    let Some(ch) = last.chars().next().filter(|_| last.chars().count() == 1) else {
        return vec![];
    };
    let n = ch as u32;
    if !(0xac00..=0xd7a3).contains(&n) {
        return vec![];
    }
    let n = n - 0xac00;
    let initial = n / 588;
    let vowel = (n % 588) / 28;
    let final_ = n % 28;
    let target = match initial {
        5 if [2, 6, 7, 12, 17, 20].contains(&vowel) => 11,
        5 if [0, 1, 8, 11, 13, 18].contains(&vowel) => 2,
        2 if [6, 12, 17, 20].contains(&vowel) => 11,
        _ => initial,
    };
    let mut result = vec![last.to_owned()];
    if target != initial {
        result.push(
            char::from_u32(0xac00 + target * 588 + vowel * 28 + final_)
                .unwrap()
                .to_string(),
        );
    }
    result
}
impl Dictionary {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let data: Dataset =
            serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
        let mut keys = HashMap::new();
        let mut first: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in data.entries.iter().enumerate() {
            if e.senses.is_empty() || e.reading.is_empty() {
                return Err("사전 항목에 뜻 또는 연결 표기가 없습니다".into());
            }
            for key in [&e.label, &e.reading].into_iter().chain(e.aliases.iter()) {
                if let Some(old) = keys.insert(normalize(key), i)
                    && old != i
                {
                    return Err("사전 검색 키 충돌".into());
                }
            }
            first.entry(e.first_syllable.clone()).or_default().push(i);
        }
        let seeds = [
            "사과",
            "나무",
            "바다",
            "학교",
            "기차",
            "고래",
            "하늘",
            "우유",
            "모자",
            "사자",
            "구름",
            "나비",
            "과자",
            "의자",
            "다리",
            "나라",
            "소나무",
            "바람",
        ]
        .iter()
        .filter_map(|s| keys.get(*s).copied())
        .collect();
        Ok(Self {
            version: data.version,
            entries: data.entries,
            keys,
            first,
            seeds,
        })
    }
    pub fn lookup(&self, input: &str) -> Option<usize> {
        self.keys.get(&normalize(input)).copied()
    }
    pub fn follows(&self, prev: usize, next: usize) -> bool {
        allowed_starts(&self.entries[prev].last_syllable)
            .contains(&self.entries[next].first_syllable)
    }
    pub fn has_next(&self, prev: usize, used: &HashSet<usize>) -> bool {
        allowed_starts(&self.entries[prev].last_syllable)
            .iter()
            .any(|s| {
                self.first
                    .get(s)
                    .is_some_and(|ids| ids.iter().any(|i| !used.contains(i)))
            })
    }
    pub fn seed(&self, used: &HashSet<usize>) -> Option<usize> {
        let offset = uuid::Uuid::new_v4().as_u128() as usize;
        self.seeds
            .iter()
            .cycle()
            .skip(offset % self.seeds.len().max(1))
            .take(self.seeds.len())
            .copied()
            .find(|i| !used.contains(i) && self.has_next(*i, used))
            .or_else(|| {
                (0..self.entries.len()).find(|i| !used.contains(i) && self.has_next(*i, used))
            })
    }
    pub fn word(&self, i: usize) -> Word {
        let e = &self.entries[i];
        let mut categories = HashSet::new();
        let meanings = e
            .senses
            .iter()
            .filter(|s| categories.insert(s.category.clone()))
            .take(5)
            .map(|s| Meaning {
                category: s.category.clone(),
                definition: s.definition.chars().take(350).collect(),
                reason: s.acceptance_reason.clone(),
                source: s.source.clone(),
            })
            .collect();
        Word {
            label: e.label.clone(),
            reading: e.reading.clone(),
            next_starts: allowed_starts(&e.last_syllable),
            meanings,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_station_names_have_explanations_and_chain_to_yeok() {
        let d = Dictionary::load("data/dictionary.json").unwrap();
        for name in ["서울역", "부산역", "강남역", "홍대입구역"] {
            let i = d.lookup(name).unwrap();
            assert_eq!(d.entries[i].last_syllable, "역");
            assert!(
                d.word(i)
                    .meanings
                    .iter()
                    .any(|m| m.category == "station-name" && !m.definition.is_empty())
            );
            assert!(d.follows(i, d.lookup("역사").unwrap()));
        }
        assert!(d.lookup("존재하지않는가짜역").is_none());
    }
    #[test]
    fn dueum_is_forward_only() {
        assert_eq!(allowed_starts("력"), vec!["력", "역"]);
        assert_eq!(allowed_starts("락"), vec!["락", "낙"]);
        assert_eq!(allowed_starts("녀"), vec!["녀", "여"]);
        assert_eq!(allowed_starts("역"), vec!["역"]);
        assert_eq!(allowed_starts("렁"), vec!["렁"]);
        assert!(allowed_starts("ab").is_empty());
    }
    #[test]
    fn normalization_and_real_dictionary() {
        let d = Dictionary::load("data/dictionary.json").unwrap();
        for word in ["사과", "아리", "원펀맨", "아처", "쉔"] {
            assert!(d.lookup(word).is_some());
        }
        assert_eq!(d.lookup("P.E.K.K.A"), d.lookup("페카"));
        assert_eq!(d.lookup("리 신"), d.lookup("리신"));
        assert_eq!(normalize("사과!"), "사과");
        assert!(d.lookup("사과🍎").is_none());
        assert!(d.follows(d.lookup("사과").unwrap(), d.lookup("과자").unwrap()));
        assert_eq!(d.word(d.lookup("기사").unwrap()).meanings.len(), 2);
    }
}

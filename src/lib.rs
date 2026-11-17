use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fs, path::Path};
use thiserror::Error;

const MINUTES_PER_DAY: i32 = 24 * 60;

#[derive(Debug, Error)]
pub enum FairtimeError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("could not parse {path} as TOML: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub title: Option<String>,
    #[serde(default = "default_occurrences")]
    pub occurrences: usize,
    #[serde(default = "default_step")]
    pub step_minutes: u16,
    #[serde(default = "default_rotate")]
    pub rotate: bool,
    #[serde(default = "default_maximum_outside")]
    pub maximum_outside_minutes: u16,
    pub participants: Vec<ParticipantConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantConfig {
    pub name: String,
    pub utc_offset: String,
    pub preferred_start: String,
    pub preferred_end: String,
    #[serde(default = "default_weight")]
    pub weight: u16,
}

#[derive(Debug, Clone)]
struct Participant {
    name: String,
    offset_minutes: i32,
    preferred_start: i32,
    preferred_end: i32,
    weight: u16,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Plan {
    pub title: String,
    pub objective: Objective,
    pub meetings: Vec<Meeting>,
    pub participants: Vec<ParticipantSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Objective {
    pub maximum_weighted_burden: u64,
    pub burden_spread: u64,
    pub total_weighted_burden: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Meeting {
    pub occurrence: usize,
    pub utc: String,
    pub local_times: Vec<LocalTime>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalTime {
    pub name: String,
    pub local: String,
    pub outside_preference_minutes: u16,
    pub weighted_burden: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ParticipantSummary {
    pub name: String,
    pub total_outside_preference_minutes: u64,
    pub total_weighted_burden: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Score {
    maximum: u64,
    spread: u64,
    total: u64,
    immediate: u64,
    utc_minute: i32,
}

impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.maximum,
            self.spread,
            self.total,
            self.immediate,
            self.utc_minute,
        )
            .cmp(&(
                other.maximum,
                other.spread,
                other.total,
                other.immediate,
                other.utc_minute,
            ))
    }
}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn default_occurrences() -> usize {
    4
}

fn default_step() -> u16 {
    30
}

fn default_rotate() -> bool {
    true
}

fn default_weight() -> u16 {
    1
}

fn default_maximum_outside() -> u16 {
    240
}

pub fn load_config(path: &Path) -> Result<Config, FairtimeError> {
    let display = path.display().to_string();
    let text = fs::read_to_string(path).map_err(|source| FairtimeError::Read {
        path: display.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| FairtimeError::Parse {
        path: display,
        source,
    })
}

pub fn plan(config: &Config) -> Result<Plan, FairtimeError> {
    let participants = validate(config)?;
    let candidates: Vec<i32> = (0..MINUTES_PER_DAY)
        .step_by(usize::from(config.step_minutes))
        .filter(|utc_minute| {
            participants.iter().all(|participant| {
                penalty_for(*utc_minute, participant) <= config.maximum_outside_minutes
            })
        })
        .collect();
    if candidates.is_empty() {
        return Err(FairtimeError::Invalid(format!(
            "no slot satisfies maximum_outside_minutes = {}; widen the limit or preferred windows",
            config.maximum_outside_minutes
        )));
    }
    if config.rotate && config.occurrences > candidates.len() {
        return Err(FairtimeError::Invalid(format!(
            "occurrences ({}) exceeds the {} distinct acceptable slots available at a {}-minute step",
            config.occurrences,
            candidates.len(),
            config.step_minutes
        )));
    }

    let mut totals = vec![0_u64; participants.len()];
    let mut raw_totals = vec![0_u64; participants.len()];
    let mut selected = Vec::with_capacity(config.occurrences);

    for _ in 0..config.occurrences {
        let mut best: Option<(Score, i32, Vec<u16>)> = None;
        for &utc_minute in &candidates {
            if config.rotate && selected.contains(&utc_minute) {
                continue;
            }
            let penalties: Vec<u16> = participants
                .iter()
                .map(|participant| penalty_for(utc_minute, participant))
                .collect();
            let projected: Vec<u64> = participants
                .iter()
                .zip(&penalties)
                .zip(&totals)
                .map(|((participant, penalty), total)| {
                    total + u64::from(*penalty) * u64::from(participant.weight)
                })
                .collect();
            let maximum = projected.iter().copied().max().unwrap_or_default();
            let minimum = projected.iter().copied().min().unwrap_or_default();
            let score = Score {
                maximum,
                spread: maximum - minimum,
                total: projected.iter().sum(),
                immediate: penalties
                    .iter()
                    .zip(&participants)
                    .map(|(penalty, participant)| {
                        u64::from(*penalty) * u64::from(participant.weight)
                    })
                    .sum(),
                utc_minute,
            };
            if best
                .as_ref()
                .is_none_or(|(best_score, _, _)| score < *best_score)
            {
                best = Some((score, utc_minute, penalties));
            }
        }

        let (_, utc_minute, penalties) = best.expect("validated candidate set cannot be empty");
        for (index, penalty) in penalties.iter().enumerate() {
            raw_totals[index] += u64::from(*penalty);
            totals[index] += u64::from(*penalty) * u64::from(participants[index].weight);
        }
        selected.push(utc_minute);
    }

    let meetings = selected
        .iter()
        .enumerate()
        .map(|(index, utc_minute)| Meeting {
            occurrence: index + 1,
            utc: format_time(*utc_minute),
            local_times: participants
                .iter()
                .map(|participant| {
                    let penalty = penalty_for(*utc_minute, participant);
                    LocalTime {
                        name: participant.name.clone(),
                        local: format_time(utc_minute + participant.offset_minutes),
                        outside_preference_minutes: penalty,
                        weighted_burden: u64::from(penalty) * u64::from(participant.weight),
                    }
                })
                .collect(),
        })
        .collect();

    let maximum = totals.iter().copied().max().unwrap_or_default();
    let minimum = totals.iter().copied().min().unwrap_or_default();
    Ok(Plan {
        title: config
            .title
            .clone()
            .unwrap_or_else(|| "Fair rotating schedule".to_owned()),
        objective: Objective {
            maximum_weighted_burden: maximum,
            burden_spread: maximum - minimum,
            total_weighted_burden: totals.iter().sum(),
        },
        meetings,
        participants: participants
            .iter()
            .enumerate()
            .map(|(index, participant)| ParticipantSummary {
                name: participant.name.clone(),
                total_outside_preference_minutes: raw_totals[index],
                total_weighted_burden: totals[index],
            })
            .collect(),
    })
}

pub fn render_text(plan: &Plan) -> String {
    let mut output = format!("{}\n\n", plan.title);
    for meeting in &plan.meetings {
        output.push_str(&format!("#{}  {} UTC", meeting.occurrence, meeting.utc));
        for local in &meeting.local_times {
            output.push_str(&format!(
                "  |  {} {} ({})",
                local.name,
                local.local,
                if local.outside_preference_minutes == 0 {
                    "preferred".to_owned()
                } else {
                    format!("+{}m", local.outside_preference_minutes)
                }
            ));
        }
        output.push('\n');
    }
    output.push_str(&format!(
        "\nmax burden: {}  |  spread: {}  |  total: {}\n",
        plan.objective.maximum_weighted_burden,
        plan.objective.burden_spread,
        plan.objective.total_weighted_burden
    ));
    output
}

fn validate(config: &Config) -> Result<Vec<Participant>, FairtimeError> {
    if config.participants.is_empty() {
        return Err(FairtimeError::Invalid(
            "at least one participant is required".to_owned(),
        ));
    }
    if config.occurrences == 0 {
        return Err(FairtimeError::Invalid(
            "occurrences must be greater than zero".to_owned(),
        ));
    }
    if config.step_minutes == 0 || 60 % config.step_minutes != 0 {
        return Err(FairtimeError::Invalid(
            "step_minutes must be a non-zero divisor of 60".to_owned(),
        ));
    }
    if config.maximum_outside_minutes > 12 * 60 {
        return Err(FairtimeError::Invalid(
            "maximum_outside_minutes cannot exceed 720".to_owned(),
        ));
    }

    config
        .participants
        .iter()
        .map(|participant| {
            if participant.name.trim().is_empty() {
                return Err(FairtimeError::Invalid(
                    "participant names cannot be empty".to_owned(),
                ));
            }
            if participant.weight == 0 {
                return Err(FairtimeError::Invalid(format!(
                    "{} has weight 0; weights must be positive",
                    participant.name
                )));
            }
            let offset_minutes = parse_offset(&participant.utc_offset)?;
            let preferred_start = parse_time(&participant.preferred_start, "preferred_start")?;
            let preferred_end = parse_time(&participant.preferred_end, "preferred_end")?;
            if preferred_start == preferred_end {
                return Err(FairtimeError::Invalid(format!(
                    "{} has an empty preferred window",
                    participant.name
                )));
            }
            Ok(Participant {
                name: participant.name.clone(),
                offset_minutes,
                preferred_start,
                preferred_end,
                weight: participant.weight,
            })
        })
        .collect()
}

fn parse_time(value: &str, field: &str) -> Result<i32, FairtimeError> {
    let (hour, minute) = value.split_once(':').ok_or_else(|| {
        FairtimeError::Invalid(format!("{field} must use 24-hour HH:MM format: {value}"))
    })?;
    let hour: i32 = hour.parse().map_err(|_| {
        FairtimeError::Invalid(format!("{field} contains an invalid hour: {value}"))
    })?;
    let minute: i32 = minute.parse().map_err(|_| {
        FairtimeError::Invalid(format!("{field} contains invalid minutes: {value}"))
    })?;
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) {
        return Err(FairtimeError::Invalid(format!(
            "{field} is outside 00:00..23:59: {value}"
        )));
    }
    Ok(hour * 60 + minute)
}

fn parse_offset(value: &str) -> Result<i32, FairtimeError> {
    let sign = match value.as_bytes().first() {
        Some(b'+') => 1,
        Some(b'-') => -1,
        _ => {
            return Err(FairtimeError::Invalid(format!(
                "UTC offset must begin with + or -: {value}"
            )));
        }
    };
    let minutes = parse_time(&value[1..], "utc_offset")?;
    if minutes > 14 * 60 {
        return Err(FairtimeError::Invalid(format!(
            "UTC offset is outside -14:00..+14:00: {value}"
        )));
    }
    Ok(sign * minutes)
}

fn penalty_for(utc_minute: i32, participant: &Participant) -> u16 {
    let local = normalize(utc_minute + participant.offset_minutes);
    if in_window(
        local,
        participant.preferred_start,
        participant.preferred_end,
    ) {
        return 0;
    }
    let to_start = circular_distance(local, participant.preferred_start);
    let to_end = circular_distance(local, participant.preferred_end);
    u16::try_from(to_start.min(to_end)).expect("daily minute distance fits in u16")
}

fn in_window(value: i32, start: i32, end: i32) -> bool {
    if start < end {
        (start..=end).contains(&value)
    } else {
        value >= start || value <= end
    }
}

fn circular_distance(left: i32, right: i32) -> i32 {
    let direct = (left - right).abs();
    direct.min(MINUTES_PER_DAY - direct)
}

fn normalize(minutes: i32) -> i32 {
    minutes.rem_euclid(MINUTES_PER_DAY)
}

fn format_time(minutes: i32) -> String {
    let minutes = normalize(minutes);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn participant(name: &str, offset: &str, start: &str, end: &str) -> ParticipantConfig {
        ParticipantConfig {
            name: name.to_owned(),
            utc_offset: offset.to_owned(),
            preferred_start: start.to_owned(),
            preferred_end: end.to_owned(),
            weight: 1,
        }
    }

    #[test]
    fn selects_shared_preferred_time_when_one_exists() {
        let config = Config {
            title: None,
            occurrences: 1,
            step_minutes: 30,
            rotate: true,
            maximum_outside_minutes: 240,
            participants: vec![
                participant("Toronto", "-05:00", "09:00", "17:00"),
                participant("London", "+00:00", "09:00", "17:00"),
            ],
        };
        let plan = plan(&config).unwrap();
        assert_eq!(plan.meetings[0].utc, "14:00");
        assert_eq!(plan.objective.total_weighted_burden, 0);
    }

    #[test]
    fn rotation_distributes_unavoidable_burden() {
        let config = Config {
            title: None,
            occurrences: 2,
            step_minutes: 60,
            rotate: true,
            maximum_outside_minutes: 480,
            participants: vec![
                participant("West", "-08:00", "09:00", "10:00"),
                participant("East", "+08:00", "09:00", "10:00"),
            ],
        };
        let plan = plan(&config).unwrap();
        let burdens: Vec<_> = plan
            .participants
            .iter()
            .map(|participant| participant.total_weighted_burden)
            .collect();
        assert_eq!(burdens[0], burdens[1]);
        assert_ne!(plan.meetings[0].utc, plan.meetings[1].utc);
    }

    #[test]
    fn supports_half_hour_offsets_and_overnight_windows() {
        let config = Config {
            title: None,
            occurrences: 1,
            step_minutes: 30,
            rotate: false,
            maximum_outside_minutes: 240,
            participants: vec![participant("Night", "+05:30", "22:00", "06:00")],
        };
        let plan = plan(&config).unwrap();
        assert_eq!(plan.meetings[0].utc, "00:00");
        assert_eq!(plan.meetings[0].local_times[0].local, "05:30");
        assert_eq!(plan.objective.total_weighted_burden, 0);
    }

    #[test]
    fn rejects_unknown_config_fields() {
        let error = toml::from_str::<Config>(
            r#"
            occurrences = 2
            mystery = true
            participants = []
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_a_schedule_when_the_hard_limit_is_impossible() {
        let config = Config {
            title: None,
            occurrences: 1,
            step_minutes: 60,
            rotate: false,
            maximum_outside_minutes: 0,
            participants: vec![
                participant("West", "-08:00", "09:00", "10:00"),
                participant("East", "+08:00", "09:00", "10:00"),
            ],
        };
        let error = plan(&config).unwrap_err();
        assert!(error.to_string().contains("no slot satisfies"));
    }

    #[test]
    fn text_output_contains_schedule_and_objective() {
        let plan = Plan {
            title: "Team".to_owned(),
            objective: Objective {
                maximum_weighted_burden: 10,
                burden_spread: 5,
                total_weighted_burden: 15,
            },
            meetings: vec![Meeting {
                occurrence: 1,
                utc: "13:00".to_owned(),
                local_times: vec![LocalTime {
                    name: "A".to_owned(),
                    local: "09:00".to_owned(),
                    outside_preference_minutes: 0,
                    weighted_burden: 0,
                }],
            }],
            participants: Vec::new(),
        };
        let text = render_text(&plan);
        assert!(text.contains("#1  13:00 UTC"));
        assert!(text.contains("max burden: 10"));
    }
}

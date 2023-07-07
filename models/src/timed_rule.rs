use chrono::{Datelike, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

use crate::{set_point::SetPoint, mixer::Mixer, hvac_request::HvacRequest};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct TimedRuleSet {
    pub rules: Vec<TimedRule>,
    pub threshold: f32,
}

impl TimedRuleSet {
    pub fn new(mut rules: Vec<TimedRule>, threshold: f32) -> Self {
        rules.sort_by_key(|rule| rule.start_time);
        TimedRuleSet { rules, threshold }
    }

    /*pub async fn load(redis: &RedisConn) -> TimedRuleSet {
        let data = {
            let mut redis = redis.get();
            if let Ok(data) = redis.get(CURRENT_RULESET_KEY).await {
                data
            } else {
                let _: Option<()> = redis.set(CURRENT_RULESET_KEY, DEFAULT_CONFIG).await.ok();
                DEFAULT_CONFIG.to_string()
            }
        };

        let mut ruleset: TimedRuleSet = serde_json::from_str(&data).ok().unwrap_or_default();
        ruleset.rules.sort_by_key(|rule| rule.start_time);
        ruleset
    }*/

    pub async fn evaluate(&self, state: &impl Mixer) -> Option<HvacRequest> {
        let rule = self.find_applicable_rule()?;

        let (mut on_weight, mut off_weight) = (0.0, 0.0);
        let mut total_points = 0;
        for set_point in &rule.set_points {
            let (heat_weight, cool_weight) = set_point.evaluate(state).await;
            total_points += 1;
            match state.mode() {
                HvacRequest::Off => off_weight += heat_weight + cool_weight, // lol
                HvacRequest::Heat => {
                    on_weight += heat_weight;
                    off_weight += cool_weight;
                }
                HvacRequest::Cool => {
                    on_weight += cool_weight;
                    off_weight += heat_weight;
                }
            }
        }
        if total_points > 0 {
            on_weight /= total_points as f32;
            off_weight /= total_points as f32;
        }

        if on_weight > off_weight && on_weight > self.threshold {
            Some(state.mode())
        } else if off_weight > on_weight && off_weight > self.threshold {
            Some(HvacRequest::Off)
        } else {
            None
        }
    }

    pub fn find_applicable_rule(&self) -> Option<&TimedRule> {
        let now = chrono::Local::now();
        let time_of_day = now.time();
        let today = now.weekday();

        if let Some(index) = self.first_rule_index_for(today) {
            // If the first rule today doesn't begin until after now, use
            // the last rule from a previous day.
            if self.rules[index].start_time > time_of_day {
                return self.last_rule_before(today);
            }

            // Find the last rule today that begins before now
            let mut result = index;
            for i in index..self.rules.len() {
                if !self.rules[i].days_enabled.enabled(today) {
                    continue;
                } else if self.rules[i].start_time <= time_of_day {
                    result = i;
                } else {
                    break;
                }
            }

            Some(&self.rules[result])
        } else {
            self.last_rule_before(today)
        }
    }

    fn first_rule_index_for(&self, day: Weekday) -> Option<usize> {
        self.rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| rule.days_enabled.enabled(day))
            .map(|(i, _)| i)
            .next()
    }

    fn last_rule_for(&self, day: Weekday) -> Option<&TimedRule> {
        self.rules
            .iter()
            .rev()
            .find(|rule| rule.days_enabled.enabled(day))
    }

    fn last_rule_before(&self, day: Weekday) -> Option<&TimedRule> {
        let mut curr = day;
        loop {
            curr = curr.pred();
            if curr == day {
                return None;
            }

            if let Some(rule) = self.last_rule_for(curr) {
                return Some(rule);
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct TimedRule {
    pub set_points: Vec<SetPoint>,
    pub start_time: NaiveTime,
    pub days_enabled: DaySet,
}

#[derive(Default, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaySet(u8);

impl DaySet {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn all() -> Self {
        DaySet(u8::MAX)
    }

    pub fn enabled(&self, day: Weekday) -> bool {
        self.0 & Self::flag_for(day) != 0
    }

    pub fn enable(&mut self, day: Weekday) {
        self.0 |= Self::flag_for(day)
    }

    pub fn disable(&mut self, day: Weekday) {
        self.0 &= !Self::flag_for(day)
    }

    fn flag_for(day: Weekday) -> u8 {
        1u8 << day.num_days_from_sunday()
    }
}

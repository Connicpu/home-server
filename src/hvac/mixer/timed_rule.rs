use redis::AsyncCommands;

use crate::RedisConn;

pub use models::timed_rule::{DaySet, TimedRule, TimedRuleSet};

pub const CURRENT_RULESET_KEY: &str = "thermostat.config.timedruleset";
const DEFAULT_CONFIG: &str = "{\"rules\":[
    {\"set_points\":[{\"min_temp\":22.0,\"max_temp\":22.5,\"probe\":\"primary\",\"weight\":1.0}],
    \"start_time\":\"06:00:00\",\"days_enabled\":255},
    {\"set_points\":[{\"min_temp\":20.0,\"max_temp\":20.5,\"probe\":\"primary\",\"weight\":1.0}],
    \"start_time\":\"09:00:00\",\"days_enabled\":255},
    {\"set_points\":[{\"min_temp\":18.0,\"max_temp\":19.0,\"probe\":\"primary\",\"weight\":1.0}],
    \"start_time\":\"21:00:00\",\"days_enabled\":255}
    ],\"threshold\":0.05}";

pub async fn load(redis: &RedisConn) -> TimedRuleSet {
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
}

use redis::AsyncCommands;
use warp::{
    filters::{path, BoxedFilter},
    Filter, Rejection, Reply,
};

use crate::{
    error::WebErrorExt,
    hvac::{
        mixer::timed_rule::{TimedRuleSet, CURRENT_RULESET_KEY},
        HvacState,
    },
    RedisConn,
};

pub async fn routes(redis: &RedisConn, hvac: &HvacState) -> BoxedFilter<(impl Reply,)> {
    const SAVED_RULES: &str = "thermostat.config.savedrules";

    let current = {
        let hvac = hvac.clone();
        warp::path("current")
            .and(path::end())
            .and(warp::get())
            .and_then(move || {
                let hvac = hvac.clone();
                async move {
                    let state = hvac.mixer.state();
                    serde_json::to_string(&*state.timed_ruleset).reject_err()
                }
            })
    };

    let set_current = {
        let hvac = hvac.clone();
        let redis = redis.clone();
        let activate_rule = redis::Script::new(&format!(
            r#"
            local ruleset = redis.call('HGET', '{SAVED_RULES}', ARGV[1])
            if ruleset ~= nil then
                redis.call('SET', '{CURRENT_RULESET_KEY}', ruleset)
                return 1
            else
                return 0
            end
        "#
        ));
        warp::path!("current" / "set" / String)
            .and(path::end())
            .and_then(move |rule| {
                let hvac = hvac.clone();
                let redis = redis.clone();
                let activate_rule = activate_rule.clone();
                async move {
                    let mut redis = redis.get();
                    let success: bool = activate_rule
                        .arg(rule)
                        .invoke_async(&mut redis)
                        .await
                        .reject_err()?;

                    if success {
                        hvac.mixer.reload_timed_rules().await;
                        Ok("ok".to_string())
                    } else {
                        Err(warp::reject::not_found())
                    }
                }
            })
    };

    let active_rule = {
        let hvac = hvac.clone();
        warp::path("active_rule")
            .and(warp::get())
            .and_then(move || {
                let hvac = hvac.clone();
                async move {
                    serde_json::to_string(&hvac.mixer.state().timed_ruleset.find_applicable_rule())
                        .reject_err()
                }
            })
    };

    let saved_rules = {
        let redis = redis.clone();
        warp::path("saved_rules")
            .and(path::end())
            .and(warp::get())
            .and_then(move || {
                let redis = redis.clone();
                async move {
                    let mut redis = redis.get();
                    let list: Vec<String> = redis.hkeys(SAVED_RULES).await.reject_err()?;

                    serde_json::to_string(&list).reject_err()
                }
            })
    };

    let get_saved_rule = {
        let redis = redis.clone();
        warp::path!("saved_rules" / String)
            .and(path::end())
            .and(warp::get())
            .and_then(move |name| {
                let redis = redis.clone();
                async move {
                    let mut redis = redis.get();
                    let rule: String = redis.hget(SAVED_RULES, &name).await.reject_err()?;

                    Ok::<_, Rejection>(rule)
                }
            })
    };

    let put_saved_rule = {
        let redis = redis.clone();
        warp::path!("saved_rules" / String)
            .and(path::end())
            .and(warp::put())
            .and(warp::body::json::<TimedRuleSet>())
            .and_then(move |name, rule| {
                let redis = redis.clone();
                async move {
                    let data = serde_json::to_string(&rule).reject_err()?;

                    let mut redis = redis.get();
                    let _: () = redis.hset(SAVED_RULES, &name, &data).await.reject_err()?;

                    Ok::<_, Rejection>("ok".to_string())
                }
            })
    };

    current
        .or(set_current)
        .or(active_rule)
        .or(saved_rules)
        .or(get_saved_rule)
        .or(put_saved_rule)
        .boxed()
}

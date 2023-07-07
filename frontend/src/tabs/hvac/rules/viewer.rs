use chrono::Weekday;
use models::{timed_rule::{TimedRuleSet, DaySet}, set_point::{SetPoint, BasicSetPoint, GradientSetPoint}};
use sycamore::prelude::*;

use crate::models::Units;

#[derive(Prop)]
pub struct RuleViewerParams<'a> {
    pub ruleset: &'a Signal<TimedRuleSet>,
}

#[component]
pub async fn RuleViewer<'a>(cx: Scope<'a>, params: RuleViewerParams<'a>) -> View<DomNode> {
    let ruleset = params.ruleset;
    let rules = create_selector(cx, || ruleset.get().rules.clone());
    view! { cx,
        "Threshold: " (ruleset.get().threshold)
        Indexed(
            iterable = rules,
            view = |cx, rule| view! { cx,
                h4 { (rule.start_time) }
                DaySetView(days = rule.days_enabled)
                SetPointList(set_points = rule.set_points.clone())
            }
        )
    }
}

#[derive(Prop)]
struct SetPointListParams {
    set_points: Vec<SetPoint>,
}

#[component]
async fn SetPointList(cx: Scope<'_>, params: SetPointListParams) -> View<DomNode> {
    let set_points = create_selector(cx, move || params.set_points.clone());
    view! { cx,
        h5 { "Setpoints" }
        table(class = "setpoint-list") {
            tr {
                th { "Type" }
                th { "Probe" }
                th { "Weight" }
                th { "Parameters" }
            }
            Indexed(
                iterable = set_points,
                view = |cx, set_point| match set_point {
                    SetPoint::Basic(basic) => view! { cx, BasicSetPointView(set_point = basic) },
                    SetPoint::Gradient(grad) => view! { cx, GradientSetPointView(set_point = grad) },
                }
            )
        }
    }
}

#[derive(Prop)]
struct BasicSetPointParams {
    set_point: BasicSetPoint,
}

#[component]
async fn BasicSetPointView(cx: Scope<'_>, params: BasicSetPointParams) -> View<DomNode> {
    let units = use_context::<Signal<Units>>(cx);
    let display_temp = |temp: f32| match *units.get() {
        Units::Celcius => format!("{temp}°C"),
        Units::Fahrenheit => format!("{}°F", temp * 9.0 / 5.0 + 32.0),
    };
    
    let set_point = params.set_point;
    view! { cx,
        tr {
            td { "Basic" }
            td { (set_point.probe) }
            td { (set_point.weight) }
            td {
                "Min: " (display_temp(set_point.min_temp))
                ", Max: " (display_temp(set_point.max_temp))
            }
        }
    }
}

#[derive(Prop)]
struct GradientSetPointParams {
    set_point: GradientSetPoint,
}

#[component]
async fn GradientSetPointView(cx: Scope<'_>, params: GradientSetPointParams) -> View<DomNode> {
    view! { cx,
    }
}

#[derive(Prop)]
struct DaySetParams {
    days: DaySet,
}

#[component]
async fn DaySetView(cx: Scope<'_>, params: DaySetParams) -> View<DomNode> {
    let day_class = move |day: Weekday| if params.days.enabled(day) {
        "enabled"
    } else {
        ""
    };

    view! { cx,
        table(class = "day-set-table") {
            tr {
                td(class = day_class(Weekday::Sun)) { "S" }
                td(class = day_class(Weekday::Mon)) { "M" }
                td(class = day_class(Weekday::Tue)) { "T" }
                td(class = day_class(Weekday::Wed)) { "W" }
                td(class = day_class(Weekday::Thu)) { "T" }
                td(class = day_class(Weekday::Fri)) { "F" }
                td(class = day_class(Weekday::Sat)) { "S" }
            }
        }
    }
}

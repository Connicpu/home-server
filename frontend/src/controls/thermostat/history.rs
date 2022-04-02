use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

use anyhow::bail;
use chrono::{DateTime, Utc};
use plotters_canvas::CanvasBackend;
use serde::Deserialize;
use sycamore::{futures::ScopeSpawnLocal, prelude::*};
use wasm_bindgen::JsCast;
use web_sys::{window, CanvasRenderingContext2d, HtmlCanvasElement};

use crate::auth::auth_token;

#[component]
pub fn TemperatureHistory<G: Html>(cx: ScopeRef) -> View<G> {
    view! { cx,
        h2 { "History" }
        TemperatureGraph {
            probe: "primary".into()
        }
    }
}

#[derive(Prop)]
struct GraphParams {
    probe: String,
}

#[component]
async fn TemperatureGraph<G: Html>(cx: ScopeRef<'_>, params: GraphParams) -> View<G> {
    let initial_data = get_day_history(&params.probe).await.unwrap_or(vec![]);
    let data = cx.create_signal(vec![]);
    let prepared = cx.create_ref(AtomicBool::new(false));
    let canvas_node = cx.create_node_ref();

    cx.create_effect(move || {
        let data = data.get();
        let Some(canvas) = canvas_node.try_get::<DomNode>() else {
            return;
        };

        if !prepared.load(SeqCst) {
            prepare_canvas(&canvas);
        }

        render_canvas(&canvas, &data).ok();
    });

    cx.spawn_local(async move {
        data.set(initial_data);
    });

    view! { cx,
        canvas(ref=canvas_node, style="width: 100%;")
    }
}

const ASPECT_RATIO: f64 = 640.0 / 400.0;

fn prepare_canvas(canvas: &DomNode) {
    let Ok(canvas) = canvas.inner_element().dyn_into::<HtmlCanvasElement>() else {
        return;
    };

    let width: f64 = window()
        .unwrap()
        .get_computed_style(&canvas)
        .unwrap()
        .unwrap()
        .get_property_value("width")
        .unwrap()
        .trim_end_matches("px")
        .parse()
        .unwrap();
    let height = width / ASPECT_RATIO;
    canvas
        .style()
        .set_property("height", &format!("{height}px"))
        .unwrap();

    let display_factor = window().unwrap().device_pixel_ratio();
    canvas.set_width((width * display_factor) as u32);
    canvas.set_height((height * display_factor) as u32);

    let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|ctx| ctx.dyn_into::<CanvasRenderingContext2d>().ok()) else {
            return
        };
    ctx.scale(display_factor, display_factor).unwrap();
}

async fn get_day_history(probe: &str) -> anyhow::Result<Vec<(f64, f64)>> {
    let item_count = 6 * 60 * 24;
    let base = window().unwrap().origin();
    let response = reqwest::Client::new()
        .get(format!(
            "{base}/api/thermostat/probes/{probe}/history?start=0&stop={item_count}"
        ))
        .header("X-Auth", auth_token())
        .send()
        .await?;

    #[derive(Deserialize)]
    struct HistoryEntry {
        time: DateTime<Utc>,
        temp: f64,
    }

    let history: Vec<HistoryEntry> = response.json().await?;
    let now = Utc::now();

    let chart_history = history
        .iter()
        .rev()
        .map(|entry| {
            const MPH: f64 = 1000.0 * 60.0 * 60.0;
            let time_ago = entry.time - now;
            (time_ago.num_milliseconds() as f64 / MPH, entry.temp)
        })
        .collect();

    Ok(chart_history)
}

fn render_canvas(canvas: &DomNode, data: &[(f64, f64)]) -> anyhow::Result<()> {
    use plotters::prelude::*;

    let Ok(canvas) = canvas.inner_element().dyn_into::<HtmlCanvasElement>() else {
        bail!("Couldn't convert canvas to HtmlCanvasElement");
    };

    let Some(backend) = CanvasBackend::with_canvas_object(canvas) else {
        bail!("Couldn't create canvas backend");
    };

    let scaling = window().unwrap().device_pixel_ratio();
    let (w, h) = backend.get_size();
    let (w, h) = ((w as f64 / scaling) as u32, (h as f64 / scaling) as u32);
    let root = backend.into_drawing_area();
    let root = root.shrink((0, 0), (w, h));
    let root = root.margin(0, 10, 0, 10);

    let Some(temp_min) = data
        .iter()
        .map(|(_, t)| *t)
        .min_by(|a, b| a.partial_cmp(b).unwrap()) else {
            bail!("Empty data set")
        };
    let Some(temp_max) = data
        .iter()
        .map(|(_, t)| *t)
        .max_by(|a, b| a.partial_cmp(b).unwrap()) else {
            bail!("Empty data set")
        };

    root.fill(&TRANSPARENT)?;
    let mut chart = ChartBuilder::on(&root)
        .x_label_area_size(40)
        .y_label_area_size(40)
        .build_cartesian_2d(-24.0f64..0.0, (temp_min - 1.0)..(temp_max + 1.0))?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(12)
        .x_desc("Hours ago")
        .x_label_formatter(&|x| format!("{:.0}", x.abs()))
        .y_labels(5)
        .y_label_formatter(&|x| format!("{:.1}", x))
        .draw()?;

    chart.draw_series(LineSeries::new(
        data.iter().cloned(),
        &RED,
    ))?;

    Ok(())
}

use std::{
    sync::atomic::{AtomicBool, Ordering::SeqCst},
    time::Duration,
};

use anyhow::bail;
use chrono::{DateTime, Utc};
use plotters_canvas::CanvasBackend;
use serde::Deserialize;
use sycamore::{futures::spawn_local_scoped, prelude::*};
use wasm_bindgen::JsCast;
use web_sys::{window, CanvasRenderingContext2d, HtmlCanvasElement};

use crate::{auth::auth_token, models::Units};

#[component]
pub fn TemperatureHistory<G: Html>(cx: Scope) -> View<G> {
    view! { cx,
        h2 { "History" }
        TemperatureGraph(probe = "primary".into())
    }
}

#[derive(Prop)]
struct GraphParams {
    probe: String,
}

#[component]
async fn TemperatureGraph<G: Html>(cx: Scope<'_>, params: GraphParams) -> View<G> {
    let data = create_signal(cx, vec![]);
    let prepared = create_ref(cx, AtomicBool::new(false));
    let canvas_node = create_node_ref(cx);
    let units = use_context::<Signal<Units>>(cx);

    create_effect(cx, move || {
        let data = data.get();
        let Some(canvas) = canvas_node.try_get::<DomNode>() else {
            return;
        };

        if !prepared.load(SeqCst) {
            prepare_canvas(&canvas);
            prepared.store(true, SeqCst);
        }

        render_canvas(&canvas, &data, *units.get()).ok();
    });

    spawn_local_scoped(cx, async move {
        loop {
            if let Ok(new_data) = get_day_history(&params.probe).await {
                data.set(new_data);
            }
            gloo_timers::future::sleep(Duration::from_secs(10)).await;
        }
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

fn render_canvas(canvas: &DomNode, data: &[(f64, f64)], units: Units) -> anyhow::Result<()> {
    use plotters::prelude::*;

    let Ok(canvas) = canvas.inner_element().dyn_into::<HtmlCanvasElement>() else {
        bail!("Couldn't convert canvas to HtmlCanvasElement");
    };

    let unit_transform = |x: f64| match units {
        Units::Celcius => x,
        Units::Fahrenheit => x * 9.0 / 5.0 + 32.0,
    };

    // Clear the canvas
    let Some(ctx) = canvas.get_context("2d").map_err(|e| anyhow::anyhow!("JsError: {e:?}"))? else {
        anyhow::bail!("No 2D context available");
    };
    let Some(ctx) = ctx.dyn_ref::<CanvasRenderingContext2d>() else {
        anyhow::bail!("2D context is the wrong type");
    };
    ctx.clear_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);

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

    let temp_min = unit_transform(temp_min);
    let temp_max = unit_transform(temp_max);

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

    if data.is_empty() {
        return Ok(());
    }

    const WINDOW_SIZE: usize = 48;
    const CHUNK_SIZE: usize = 18;
    let chart_data = data
        // Iterate over rolling windows of the data
        .windows(WINDOW_SIZE)
        // Average the windows
        .map(|window| {
            window
                .iter()
                .cloned()
                // Compute the sum of the X and Y axis values in the window (aka time, temp)
                .fold((0.0, 0.0, 0), |(acctime, acctemp, count), (time, temp)| {
                    (acctime + time, acctemp + temp, count + 1)
                })
        })
        // Divide the (X, Y) value by the count to turn it into an average
        .map(|(acctime, acctemp, count)| (acctime / count as f64, acctemp / count as f64))
        // Add extra data at the end to ensure no data points are left out of chunking
        .chain([data[data.len() - 1]; CHUNK_SIZE - 1])
        // Divide the data points into chunks
        .array_chunks::<CHUNK_SIZE>()
        // Average the chunks just like we did the windows
        .map(|chunk| {
            chunk
                .into_iter()
                .fold((0.0, 0.0, 0), |(acctime, acctemp, count), (time, temp)| {
                    (acctime + time, acctemp + temp, count + 1)
                })
        })
        .map(|(acctime, acctemp, count)| (acctime / count as f64, acctemp / count as f64))
        // Convert to Fahrenheit if that's selected
        .map(|(time, temp)| (time, unit_transform(temp)));

    chart.draw_series(LineSeries::new(chart_data, &RED))?;

    Ok(())
}

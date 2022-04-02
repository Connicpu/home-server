use anyhow::bail;
use chrono::{DateTime, Utc};
use plotters::prelude::*;
use plotters_backend::{BackendColor, BackendStyle, BackendTextStyle, DrawingErrorKind};
use plotters_canvas::CanvasBackend;
use serde::Deserialize;
use sycamore::{futures::ScopeSpawnLocal, prelude::*};
use wasm_bindgen::JsCast;
use web_sys::{window, CanvasRenderingContext2d, HtmlCanvasElement};

use crate::auth::auth_token;

#[component]
pub fn TemperatureHistory<G: Html>(cx: ScopeRef) -> View<G> {
    let canvas_node = cx.create_node_ref();

    // Do it in the Future™️ because then it won't get executed until
    cx.spawn_local(async move {
        let canvas = canvas_node.get::<DomNode>();

        prepare_canvas(canvas.clone());

        let Ok(data) = get_day_history().await else { return };

        if let Err(err) = render_canvas(canvas.clone(), &data) {
            window()
                .unwrap()
                .alert_with_message(&format!("Error drawing canvas: {err:#?}"))
                .unwrap();
        };
    });

    view! { cx,
        h2 { "History" }
        canvas(ref=canvas_node, style="width: 100%;")
    }
}

const ASPECT_RATIO: f64 = 640.0 / 400.0;

fn prepare_canvas(canvas: DomNode) {
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

async fn get_day_history() -> anyhow::Result<Vec<(f64, f64)>> {
    let item_count = 6 * 60 * 24;
    let base = window().unwrap().origin();
    let response = reqwest::Client::new()
        .get(format!(
            "{base}/api/thermostat/probes/primary/history?start=0&stop={item_count}"
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

fn render_canvas(canvas: DomNode, data: &[(f64, f64)]) -> anyhow::Result<()> {
    use plotters::prelude::*;

    let Ok(canvas) = canvas.inner_element().dyn_into::<HtmlCanvasElement>() else {
        bail!("Couldn't convert canvas to HtmlCanvasElement");
    };

    let Some(backend) = CanvasBackend::with_canvas_object(canvas) else {
        bail!("Couldn't create canvas backend");
    };

    let backend = ScaledCanvasBackend(backend, window().unwrap().device_pixel_ratio());

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

    let root = backend.into_drawing_area();
    let root = root.margin(0, 10, 0, 10);

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

    chart.draw_series(LineSeries::new(data.iter().cloned(), &RED))?;

    Ok(())
}

struct ScaledCanvasBackend(CanvasBackend, f64);

impl DrawingBackend for ScaledCanvasBackend {
    type ErrorType = <CanvasBackend as DrawingBackend>::ErrorType;

    fn get_size(&self) -> (u32, u32) {
        let (w, h) = self.0.get_size();
        ((w as f64 / self.1) as u32, (h as f64 / self.1) as u32)
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.0.ensure_prepared()
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.0.present()
    }

    fn draw_pixel(
        &mut self,
        point: (i32, i32),
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.0.draw_pixel(point, color)
    }

    fn draw_line<S>(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>>
    where
        S: BackendStyle,
    {
        self.0.draw_line(from, to, style)
    }

    fn draw_rect<S>(
        &mut self,
        upper_left: (i32, i32),
        bottom_right: (i32, i32),
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>>
    where
        S: BackendStyle,
    {
        self.0.draw_rect(upper_left, bottom_right, style, fill)
    }

    fn draw_path<S, I>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>>
    where
        S: BackendStyle,
        I: IntoIterator<Item = (i32, i32)>,
    {
        self.0.draw_path(path, style)
    }

    fn draw_circle<S>(
        &mut self,
        center: (i32, i32),
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>>
    where
        S: BackendStyle,
    {
        self.0.draw_circle(center, radius, style, fill)
    }

    fn fill_polygon<S, I>(
        &mut self,
        vert: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>>
    where
        S: BackendStyle,
        I: IntoIterator<Item = (i32, i32)>,
    {
        self.0.fill_polygon(vert, style)
    }

    fn draw_text<TStyle>(
        &mut self,
        text: &str,
        style: &TStyle,
        pos: (i32, i32),
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>>
    where
        TStyle: BackendTextStyle,
    {
        self.0.draw_text(text, style, pos)
    }

    fn estimate_text_size<TStyle>(
        &self,
        text: &str,
        style: &TStyle,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>>
    where
        TStyle: BackendTextStyle,
    {
        self.0.estimate_text_size(text, style)
    }

    fn blit_bitmap<'a>(
        &mut self,
        pos: (i32, i32),
        (iw, ih): (u32, u32),
        src: &'a [u8],
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.0.blit_bitmap(pos, (iw, ih), src)
    }
}

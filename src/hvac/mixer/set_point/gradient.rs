use serde::{Deserialize, Serialize};

use crate::hvac::mixer::{HvacRequest, MixerState, set_point::EMPTY_REQUEST};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(from = "UnsortedGradientSetPoint")]
pub struct GradientSetPoint {
    pub probe: String,
    pub weight: f32,
    pub stop_points: Vec<StopPoint>,
}

impl GradientSetPoint {
    pub async fn evaluate(&self, state: &MixerState) -> (f32, f32) {
        match self.stop_points.len() {
            0 => return EMPTY_REQUEST,
            1 => {
                let point = &self.stop_points[0];
                return (point.heat_value * self.weight, point.cool_value * self.weight);
            }
            _ => ()
        }

        let Some(probe) = state.probes.get(&self.probe).await else {
            return EMPTY_REQUEST;
        };

        let temp = probe.value();
        
        let right_node = self.right_applicable_node(temp);
        if right_node == 0 {
            self.calculate(temp, &self.stop_points[0], &self.stop_points[1])
        } else if right_node == self.stop_points.len() {
            self.calculate(temp, &self.stop_points[right_node - 2], &self.stop_points[right_node - 1])
        } else {
            self.calculate(temp, &self.stop_points[right_node - 1], &self.stop_points[right_node])
        }
    }

    fn right_applicable_node(&self, temp: f32) -> usize {
        for (i, point) in self.stop_points.iter().enumerate() {
            if point.temp > temp {
                return i;
            }
        }
        self.stop_points.len()
    }

    fn calculate(&self, temp: f32, left: &StopPoint, right: &StopPoint) -> (f32, f32) {
        let dt = right.temp - left.temp;
        let dh = right.heat_value - left.heat_value;
        let dc = right.cool_value - left.cool_value;
        let t = (temp - left.temp) / dt;
        ((left.heat_value + dh * t) * self.weight, (left.cool_value + dc * t) * self.weight)
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub struct StopPoint {
    pub temp: f32,
    pub request: HvacRequest,
    pub heat_value: f32,
    pub cool_value: f32,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct UnsortedGradientSetPoint(GradientSetPoint);

impl From<UnsortedGradientSetPoint> for GradientSetPoint {
    fn from(mut unsorted: UnsortedGradientSetPoint) -> Self {
        unsorted.0.stop_points.retain(|point| point.temp.is_finite());
        unsorted.0.stop_points.sort_by_key(|point| cursed_float_sortable(point.temp));
        unsorted.0
    }
}

fn cursed_float_sortable(f: f32) -> impl Ord {
    let i = f32::to_bits(f) as i32;
    i ^ ((i >> 30) as u32 >> 1) as i32
}


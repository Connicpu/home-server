use serde::{Deserialize, Serialize};

use crate::mixer::Mixer;

pub use self::{basic::BasicSetPoint, gradient::GradientSetPoint};

pub mod basic;
pub mod gradient;

const EMPTY_REQUEST: (f32, f32) = (0.0, 0.0);

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", from = "TryWithDefaultDeserializeSetPoint")]
#[serde(rename_all = "snake_case")]
pub enum SetPoint {
    Basic(BasicSetPoint),
    Gradient(GradientSetPoint),
}

impl SetPoint {
    pub async fn evaluate(&self, state: &impl Mixer) -> (f32, f32) {
        match self {
            SetPoint::Basic(sp) => sp.evaluate(state).await,
            SetPoint::Gradient(sp) => sp.evaluate(state).await,
        }
    }
}

/// A Cursed Hack™ to deserialize as a "Basic" set point if no tag is specified
#[derive(Deserialize)]
#[serde(untagged)]
enum TryWithDefaultDeserializeSetPoint {
    SetPoint(TaggedSetPoint),
    Basic(BasicSetPoint),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum TaggedSetPoint {
    Basic(BasicSetPoint),
    Gradient(GradientSetPoint),
}

impl From<TryWithDefaultDeserializeSetPoint> for SetPoint {
    fn from(from: TryWithDefaultDeserializeSetPoint) -> SetPoint {
        match from {
            TryWithDefaultDeserializeSetPoint::SetPoint(sp) => match sp {
                TaggedSetPoint::Basic(sp) => SetPoint::Basic(sp),
                TaggedSetPoint::Gradient(sp) => SetPoint::Gradient(sp),
            },
            TryWithDefaultDeserializeSetPoint::Basic(basic) => SetPoint::Basic(basic),
        }
    }
}

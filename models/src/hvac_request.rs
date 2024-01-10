use std::fmt;

use serde::{Serialize, Deserialize};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HvacRequest {
    Off,
    Heat,
    Cool,
}

impl HvacRequest {
    pub fn from_payload(payload: &[u8]) -> Option<HvacRequest> {
        match payload.get(0) {
            Some(b'o' | b'O') => Some(HvacRequest::Off),
            Some(b'h' | b'H') => Some(HvacRequest::Heat),
            Some(b'c' | b'C') => Some(HvacRequest::Cool),
            _ => None,
        }
    }

    pub fn from_string(payload: String) -> Option<HvacRequest> {
        HvacRequest::from_payload(payload.as_bytes())
    }

    pub fn payload_str(self) -> &'static str {
        match self {
            HvacRequest::Off => "off",
            HvacRequest::Heat => "heat",
            HvacRequest::Cool => "cool",
        }
    }

    pub fn payload(self) -> &'static [u8] {
        self.payload_str().as_bytes()
    }
}

impl std::ops::BitAnd for HvacRequest {
    type Output = HvacRequest;
    fn bitand(self, rhs: HvacRequest) -> HvacRequest {
        match (self, rhs) {
            (HvacRequest::Heat, HvacRequest::Heat) => HvacRequest::Heat,
            (HvacRequest::Cool, HvacRequest::Cool) => HvacRequest::Cool,
            _ => HvacRequest::Off,
        }
    }
}

impl fmt::Display for HvacRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HvacRequest::Off => f.write_str("Off"),
            HvacRequest::Heat => f.write_str("Heat"),
            HvacRequest::Cool => f.write_str("Cool"),
        }
    }
}

impl Default for HvacRequest {
    fn default() -> Self {
        HvacRequest::Off
    }
}

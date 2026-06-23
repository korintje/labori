use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::{LaboriError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementMode {
    Single,
    Multi,
}

impl MeasurementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multi => "multi",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartRequest {
    pub mode: MeasurementMode,
    pub interval_seconds: f64,
    #[serde(default)]
    pub channels: Vec<u8>,
}

impl StartRequest {
    pub fn validate(mut self) -> Result<Self> {
        if !self.interval_seconds.is_finite()
            || self.interval_seconds < 0.000_01
            || self.interval_seconds > 10.0
        {
            return Err(LaboriError::Invalid(
                "interval_seconds must be between 0.00001 and 10".into(),
            ));
        }
        match self.mode {
            MeasurementMode::Single => self.channels.clear(),
            MeasurementMode::Multi => {
                self.channels.sort_unstable();
                self.channels.dedup();
                if self.channels.is_empty() || self.channels.iter().any(|channel| *channel > 5) {
                    return Err(LaboriError::Invalid(
                        "multi mode requires at least one channel from 0 through 5".into(),
                    ));
                }
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeasurementStatus {
    pub running: bool,
    pub session_id: Option<i64>,
    pub mode: Option<MeasurementMode>,
    pub interval_seconds: Option<f64>,
    pub channels: Vec<u8>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionSummary {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub mode: String,
    pub interval_seconds: f64,
    pub channels: String,
    pub state: String,
    pub sample_count: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Sample {
    pub session_id: i64,
    pub sequence: i64,
    pub channel: i64,
    pub started_ns: i64,
    pub ended_ns: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionEvent {
    pub id: i64,
    pub session_id: i64,
    pub created_at: String,
    pub at_sequence: i64,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveEvent {
    Status {
        status: MeasurementStatus,
    },
    Sample {
        sample: Sample,
    },
    Notice {
        session_id: i64,
        at_sequence: i64,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct SampleQuery {
    #[serde(default)]
    pub after_sequence: i64,
    #[serde(default = "default_sample_limit")]
    pub limit: i64,
}

fn default_sample_limit() -> i64 {
    10_000
}

impl SampleQuery {
    pub fn bounded(self) -> Self {
        Self {
            after_sequence: self.after_sequence.max(-1),
            limit: self.limit.clamp(1, 50_000),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_multi_channel_request() {
        let request = StartRequest {
            mode: MeasurementMode::Multi,
            interval_seconds: 0.001,
            channels: vec![3, 1, 3],
        }
        .validate()
        .unwrap();
        assert_eq!(request.channels, vec![1, 3]);
    }

    #[test]
    fn rejects_invalid_interval_and_channel() {
        assert!(StartRequest {
            mode: MeasurementMode::Single,
            interval_seconds: 0.0,
            channels: vec![],
        }
        .validate()
        .is_err());
        assert!(StartRequest {
            mode: MeasurementMode::Multi,
            interval_seconds: 0.1,
            channels: vec![6],
        }
        .validate()
        .is_err());
    }
}

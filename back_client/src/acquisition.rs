use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
use rppal::gpio::{Gpio, OutputPin};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedReadHalf, tcp::OwnedWriteHalf, TcpStream},
    sync::{broadcast, mpsc, oneshot, watch, RwLock},
};

use crate::{
    config::Config,
    error::{LaboriError, Result},
    model::{LiveEvent, MeasurementMode, MeasurementStatus, Sample, StartRequest},
    storage::StorageHandle,
};

#[cfg(target_os = "linux")]
const GPIO_PINS: [u8; 6] = [17, 27, 22, 23, 24, 25];

pub enum ControlCommand {
    Start {
        request: StartRequest,
        reply: oneshot::Sender<Result<MeasurementStatus>>,
    },
    Stop {
        reply: oneshot::Sender<Result<MeasurementStatus>>,
    },
}

#[derive(Clone)]
pub struct Controller {
    tx: mpsc::Sender<ControlCommand>,
    status: Arc<RwLock<MeasurementStatus>>,
}

impl Controller {
    pub async fn start(&self, request: StartRequest) -> Result<MeasurementStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ControlCommand::Start {
                request,
                reply: reply_tx,
            })
            .await
            .map_err(|_| LaboriError::ChannelClosed("measurement controller"))?;
        reply_rx
            .await
            .map_err(|_| LaboriError::ChannelClosed("measurement reply"))?
    }

    pub async fn stop(&self) -> Result<MeasurementStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ControlCommand::Stop { reply: reply_tx })
            .await
            .map_err(|_| LaboriError::ChannelClosed("measurement controller"))?;
        reply_rx
            .await
            .map_err(|_| LaboriError::ChannelClosed("measurement reply"))?
    }

    pub async fn status(&self) -> MeasurementStatus {
        self.status.read().await.clone()
    }
}

struct Completion {
    session_id: i64,
    sample_count: u64,
    degraded: bool,
    result: Result<()>,
}

pub fn spawn(
    config: Config,
    storage: StorageHandle,
    live: broadcast::Sender<LiveEvent>,
) -> Controller {
    let (tx, rx) = mpsc::channel(32);
    let status = Arc::new(RwLock::new(MeasurementStatus::default()));
    tokio::spawn(controller_loop(config, storage, live, status.clone(), rx));
    Controller { tx, status }
}

async fn controller_loop(
    config: Config,
    storage: StorageHandle,
    live: broadcast::Sender<LiveEvent>,
    status: Arc<RwLock<MeasurementStatus>>,
    mut commands: mpsc::Receiver<ControlCommand>,
) {
    let (completion_tx, mut completion_rx) = mpsc::channel::<Completion>(1);
    let mut stop_tx: Option<watch::Sender<bool>> = None;

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    ControlCommand::Start { request, reply } => {
                        if status.read().await.running {
                            let _ = reply.send(Err(LaboriError::Busy));
                            continue;
                        }
                        let request = match request.validate() {
                            Ok(request) => request,
                            Err(error) => {
                                let _ = reply.send(Err(error));
                                continue;
                            }
                        };
                        let session_id = match storage.begin(
                            request.mode,
                            request.interval_seconds,
                            request.channels.clone(),
                        ).await {
                            Ok(id) => id,
                            Err(error) => {
                                let _ = reply.send(Err(error));
                                continue;
                            }
                        };
                        let new_status = MeasurementStatus {
                            running: true,
                            session_id: Some(session_id),
                            mode: Some(request.mode),
                            interval_seconds: Some(request.interval_seconds),
                            channels: request.channels.clone(),
                            last_error: None,
                        };
                        *status.write().await = new_status.clone();
                        let _ = live.send(LiveEvent::Status { status: new_status.clone() });
                        let _ = reply.send(Ok(new_status));

                        let (new_stop_tx, stop_rx) = watch::channel(false);
                        stop_tx = Some(new_stop_tx);
                        let task_config = config.clone();
                        let task_storage = storage.clone();
                        let task_live = live.clone();
                        let task_completion = completion_tx.clone();
                        tokio::spawn(async move {
                            let (sample_count, degraded, result) = run_session(
                                task_config,
                                task_storage,
                                task_live,
                                session_id,
                                request,
                                stop_rx,
                            ).await;
                            let _ = task_completion.send(Completion {
                                session_id,
                                sample_count,
                                degraded,
                                result,
                            }).await;
                        });
                    }
                    ControlCommand::Stop { reply } => {
                        if let Some(sender) = stop_tx.as_ref() {
                            let _ = sender.send(true);
                            let mut current = status.read().await.clone();
                            current.last_error = Some("stop requested; flushing accepted samples".into());
                            let _ = reply.send(Ok(current));
                        } else {
                            let _ = reply.send(Err(LaboriError::NotRunning));
                        }
                    }
                }
            }
            completion = completion_rx.recv() => {
                let Some(completion) = completion else { break };
                stop_tx = None;
                let (state_name, error_message) = match &completion.result {
                    Ok(()) if completion.degraded => (
                        "completed_with_errors",
                        Some(
                            "measurement completed with recorded communication or timing gaps"
                                .to_string(),
                        ),
                    ),
                    Ok(()) => ("completed", None),
                    Err(error) => ("failed", Some(error.to_string())),
                };
                if let Err(error) = storage.finish(
                    completion.session_id,
                    state_name,
                    completion.sample_count,
                    error_message.clone(),
                ).await {
                    tracing::error!(%error, "failed to finalize measurement session");
                }
                let final_status = MeasurementStatus {
                    running: false,
                    session_id: None,
                    mode: None,
                    interval_seconds: None,
                    channels: Vec::new(),
                    last_error: error_message,
                };
                *status.write().await = final_status.clone();
                let _ = live.send(LiveEvent::Status { status: final_status });
            }
        }
    }
}

async fn run_session(
    config: Config,
    storage: StorageHandle,
    live: broadcast::Sender<LiveEvent>,
    session_id: i64,
    request: StartRequest,
    mut stop: watch::Receiver<bool>,
) -> (u64, bool, Result<()>) {
    let mut sequence = 0_u64;
    let mut degraded = false;
    let mut reconnect_attempts = 0_u32;
    let origin = Instant::now();
    let mut gpio = match request.mode {
        MeasurementMode::Multi => match GpioBank::new() {
            Ok(gpio) => Some(gpio),
            Err(error) => return (0, false, Err(error)),
        },
        MeasurementMode::SingleLog | MeasurementMode::SingleDirect => None,
    };
    let channels = if request.mode != MeasurementMode::Multi {
        vec![0]
    } else {
        request.channels.clone()
    };
    let mut channel_index = 0_usize;
    let mut connection: Option<Instrument> = None;
    let mut storage_health = storage.health();

    loop {
        if *stop.borrow() {
            if let Some(instrument) = connection.as_mut() {
                let _ = instrument.send(":FRUN 0").await;
            }
            return (sequence, degraded, Ok(()));
        }
        if let Some(error) = storage_health.borrow().clone() {
            return (
                sequence,
                degraded,
                Err(LaboriError::Instrument(format!(
                    "storage writer stopped: {error}"
                ))),
            );
        }
        if connection.is_none() {
            match Instrument::connect(&config, request.mode, request.interval_seconds).await {
                Ok(instrument) => {
                    connection = Some(instrument);
                    let recovered_after_attempts = reconnect_attempts;
                    reconnect_attempts = 0;
                    if sequence > 0 || recovered_after_attempts > 0 {
                        degraded = true;
                        if sequence > 0 && request.mode == MeasurementMode::SingleLog {
                            let interval_ns =
                                (request.interval_seconds * 1_000_000_000.0).round() as u64;
                            let expected_sequence =
                                (origin.elapsed().as_nanos() as u64) / interval_ns.max(1);
                            if expected_sequence > sequence {
                                let missing = expected_sequence - sequence;
                                let gap_message = format!(
                                    "estimated {missing} missing samples while disconnected"
                                );
                                if let Err(error) = storage.try_event(
                                    session_id,
                                    sequence as i64,
                                    "gap",
                                    gap_message.clone(),
                                ) {
                                    return (sequence, degraded, Err(error));
                                }
                                let _ = live.send(LiveEvent::Notice {
                                    session_id,
                                    at_sequence: sequence as i64,
                                    message: gap_message,
                                });
                                sequence = expected_sequence;
                            }
                        }
                        let message = "instrument connection restored".to_string();
                        if let Err(error) = storage.try_event(
                            session_id,
                            sequence as i64,
                            "reconnected",
                            message.clone(),
                        ) {
                            return (sequence, degraded, Err(error));
                        }
                        let _ = live.send(LiveEvent::Notice {
                            session_id,
                            at_sequence: sequence as i64,
                            message,
                        });
                    }
                }
                Err(error) => {
                    degraded = true;
                    reconnect_attempts = reconnect_attempts.saturating_add(1);
                    let message = format!("{error} (reconnect attempt {reconnect_attempts})");
                    if reconnect_attempts == 1 || reconnect_attempts.is_power_of_two() {
                        if let Err(storage_error) = storage.try_event(
                            session_id,
                            sequence as i64,
                            "connection_error",
                            message.clone(),
                        ) {
                            return (sequence, degraded, Err(storage_error));
                        }
                        let _ = live.send(LiveEvent::Notice {
                            session_id,
                            at_sequence: sequence as i64,
                            message,
                        });
                    }
                    let exponent = reconnect_attempts.saturating_sub(1).min(10);
                    let delay = config
                        .reconnect_millis
                        .saturating_mul(1_u64 << exponent)
                        .min(30_000);
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
                        changed = stop.changed() => {
                            if changed.is_err() || *stop.borrow() {
                                return (sequence, degraded, Ok(()));
                            }
                        }
                        changed = storage_health.changed() => {
                            if changed.is_err() {
                                return (sequence, degraded, Err(LaboriError::ChannelClosed("storage health")));
                            }
                        }
                    }
                    continue;
                }
            }
        }

        if request.mode == MeasurementMode::SingleLog {
            let mut stop_now = false;
            let mut storage_failed = None;
            let read_result = {
                let instrument = match connection.as_mut() {
                    Some(instrument) => instrument,
                    None => continue,
                };
                tokio::select! {
                    result = instrument.read_log(config.instrument_timeout()) => Some(result),
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            stop_now = true;
                        }
                        None
                    }
                    changed = storage_health.changed() => {
                        storage_failed = Some(if changed.is_err() {
                            "storage health channel closed".to_string()
                        } else {
                            storage_health.borrow().clone()
                                .unwrap_or_else(|| "storage writer stopped".to_string())
                        });
                        None
                    }
                }
            };
            if stop_now {
                if let Some(instrument) = connection.as_mut() {
                    let _ = instrument.send(":FRUN 0").await;
                }
                return (sequence, degraded, Ok(()));
            }
            if let Some(error) = storage_failed {
                return (sequence, degraded, Err(LaboriError::Instrument(error)));
            }
            let Some(read_result) = read_result else {
                continue;
            };
            match read_result {
                Ok(values) => {
                    let interval_ns = (request.interval_seconds * 1_000_000_000.0).round() as i64;
                    for value in values {
                        let started_ns = (sequence as i64).saturating_mul(interval_ns);
                        let sample = Sample {
                            session_id,
                            sequence: sequence as i64,
                            channel: 0,
                            started_ns,
                            ended_ns: started_ns.saturating_add(interval_ns),
                            value,
                        };
                        if let Err(error) = storage.try_sample(sample.clone()) {
                            return (sequence, degraded, Err(error));
                        }
                        let _ = live.send(LiveEvent::Sample { sample });
                        sequence += 1;
                    }
                }
                Err(error) => {
                    degraded = true;
                    connection = None;
                    let message = error.to_string();
                    if let Err(storage_error) = storage.try_event(
                        session_id,
                        sequence as i64,
                        "measurement_error",
                        message.clone(),
                    ) {
                        return (sequence, degraded, Err(storage_error));
                    }
                    let _ = live.send(LiveEvent::Notice {
                        session_id,
                        at_sequence: sequence as i64,
                        message,
                    });
                }
            }
            continue;
        }

        let channel = channels[channel_index];
        if let Some(gpio) = gpio.as_mut() {
            gpio.select(channel);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(config.gpio_settle_millis)) => {}
                changed = stop.changed() => {
                    gpio.clear();
                    if changed.is_err() || *stop.borrow() {
                        return (sequence, degraded, Ok(()));
                    }
                }
            }
        }

        let started_ns = elapsed_ns(origin);
        let mut stop_now = false;
        let mut storage_failed = None;
        let result = {
            let instrument = match connection.as_mut() {
                Some(instrument) => instrument,
                None => continue,
            };
            tokio::select! {
                result = instrument.measure(config.instrument_timeout()) => Some(result),
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        stop_now = true;
                    }
                    None
                }
                changed = storage_health.changed() => {
                    storage_failed = Some(if changed.is_err() {
                        "storage health channel closed".to_string()
                    } else {
                        storage_health.borrow().clone()
                            .unwrap_or_else(|| "storage writer stopped".to_string())
                    });
                    None
                }
            }
        };
        if stop_now {
            if let Some(gpio) = gpio.as_mut() {
                gpio.clear();
            }
            if let Some(instrument) = connection.as_mut() {
                let _ = instrument.send(":FRUN 0").await;
            }
            return (sequence, degraded, Ok(()));
        }
        if let Some(error) = storage_failed {
            return (sequence, degraded, Err(LaboriError::Instrument(error)));
        }
        let Some(result) = result else {
            continue;
        };
        let ended_ns = elapsed_ns(origin);
        if let Some(gpio) = gpio.as_mut() {
            gpio.clear();
        }

        match result {
            Ok(value) => {
                let sample = Sample {
                    session_id,
                    sequence: sequence as i64,
                    channel: channel as i64,
                    started_ns,
                    ended_ns,
                    value,
                };
                if let Err(error) = storage.try_sample(sample.clone()) {
                    return (sequence, degraded, Err(error));
                }
                let _ = live.send(LiveEvent::Sample { sample });
                sequence += 1;
                if request.mode == MeasurementMode::Multi {
                    channel_index = (channel_index + 1) % channels.len();
                }
            }
            Err(error) => {
                degraded = true;
                connection = None;
                let message = error.to_string();
                if let Err(storage_error) = storage.try_event(
                    session_id,
                    sequence as i64,
                    "measurement_error",
                    message.clone(),
                ) {
                    return (sequence, degraded, Err(storage_error));
                }
                let _ = live.send(LiveEvent::Notice {
                    session_id,
                    at_sequence: sequence as i64,
                    message,
                });
            }
        }
    }
}

struct Instrument {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Instrument {
    async fn connect(
        config: &Config,
        mode: MeasurementMode,
        interval_seconds: f64,
    ) -> Result<Self> {
        let stream = tokio::time::timeout(
            config.instrument_timeout(),
            TcpStream::connect(&config.device_addr),
        )
        .await
        .map_err(|_| LaboriError::Instrument("connection timed out".into()))?
        .map_err(|error| LaboriError::Instrument(error.to_string()))?;
        stream.set_nodelay(true)?;
        let (reader, writer) = stream.into_split();
        let mut instrument = Self {
            reader: BufReader::new(reader),
            writer,
        };
        instrument
            .send(&format!(":FUNC {}", config.measurement_function))
            .await?;
        let actual_function = instrument
            .query(":FUNC?", config.instrument_timeout())
            .await?;
        if actual_function.trim() != config.measurement_function {
            return Err(LaboriError::Instrument(format!(
                "instrument function verification failed: requested {}, got {:?}",
                config.measurement_function, actual_function
            )));
        }
        instrument
            .send(&format!(":GATE:TIME {}", gate_value(interval_seconds)))
            .await?;
        let actual_interval = instrument
            .query(":GATE:TIME?", config.instrument_timeout())
            .await?
            .trim()
            .parse::<f64>()
            .map_err(|_| LaboriError::Instrument("invalid GATE:TIME? response".into()))?;
        let tolerance = interval_seconds.abs().max(1.0) * 1e-9;
        if (actual_interval - interval_seconds).abs() > tolerance {
            return Err(LaboriError::Instrument(format!(
                "gate time verification failed: requested {interval_seconds}, got {actual_interval}"
            )));
        }
        match mode {
            MeasurementMode::SingleLog => {
                instrument.send(":FRUN 0").await?;
                instrument.send(":LOG:LEN 5e5").await?;
                instrument.send(":LOG:CLE").await?;
                instrument.send(":FRUN 1").await?;
            }
            MeasurementMode::SingleDirect | MeasurementMode::Multi => {
                instrument.send(":FRUN 0").await?
            }
        }
        Ok(instrument)
    }

    async fn send(&mut self, command: &str) -> Result<()> {
        self.writer.write_all(command.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn measure(&mut self, timeout: Duration) -> Result<f64> {
        let line = self.query(":MEAS?", timeout).await?;
        line.trim()
            .parse::<f64>()
            .map_err(|_| LaboriError::Instrument(format!("invalid measurement response: {line:?}")))
    }

    async fn read_log(&mut self, timeout: Duration) -> Result<Vec<f64>> {
        let line = self.query(":LOG:DATA?", timeout).await?;
        let mut values = Vec::new();
        for field in line.trim().split(',').filter(|field| !field.is_empty()) {
            values.push(field.parse::<f64>().map_err(|_| {
                LaboriError::Instrument(format!("invalid log response field: {field:?}"))
            })?);
        }
        Ok(values)
    }

    async fn query(&mut self, command: &str, timeout: Duration) -> Result<String> {
        self.send(command).await?;
        let mut line = String::with_capacity(1024);
        let bytes = tokio::time::timeout(timeout, self.reader.read_line(&mut line))
            .await
            .map_err(|_| LaboriError::Instrument(format!("{command} response timed out")))??;
        if bytes == 0 {
            return Err(LaboriError::Instrument(
                "instrument closed the connection".into(),
            ));
        }
        Ok(line)
    }
}

#[cfg(target_os = "linux")]
struct GpioBank {
    pins: Vec<OutputPin>,
}

#[cfg(target_os = "linux")]
impl GpioBank {
    fn new() -> Result<Self> {
        let gpio = Gpio::new().map_err(|error| LaboriError::Instrument(error.to_string()))?;
        let mut pins = Vec::with_capacity(GPIO_PINS.len());
        for number in GPIO_PINS {
            let mut pin = gpio
                .get(number)
                .map_err(|error| LaboriError::Instrument(error.to_string()))?
                .into_output_low();
            pin.set_low();
            pins.push(pin);
        }
        Ok(Self { pins })
    }

    fn select(&mut self, channel: u8) {
        self.clear();
        self.pins[channel as usize].set_high();
    }

    fn clear(&mut self) {
        for pin in &mut self.pins {
            pin.set_low();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for GpioBank {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(not(target_os = "linux"))]
struct GpioBank;

#[cfg(not(target_os = "linux"))]
impl GpioBank {
    fn new() -> Result<Self> {
        Err(LaboriError::Instrument(
            "multi-channel GPIO measurement is supported only on Linux/Raspberry Pi".into(),
        ))
    }

    fn select(&mut self, _channel: u8) {}
    fn clear(&mut self) {}
}

fn elapsed_ns(origin: Instant) -> i64 {
    origin.elapsed().as_nanos().min(i64::MAX as u128) as i64
}

fn gate_value(seconds: f64) -> String {
    match seconds {
        value if (value - 0.000_01).abs() < f64::EPSILON => "10E-6".into(),
        value if (value - 0.000_1).abs() < f64::EPSILON => "0.10E-3".into(),
        value if (value - 0.001).abs() < f64::EPSILON => "1.0E-3".into(),
        value if (value - 0.01).abs() < f64::EPSILON => "10E-3".into(),
        value if (value - 0.1).abs() < f64::EPSILON => "0.10E+0".into(),
        value if (value - 1.0).abs() < f64::EPSILON => "1.0E+0".into(),
        value if (value - 10.0).abs() < f64::EPSILON => "10.0E+0".into(),
        value => value.to_string(),
    }
}

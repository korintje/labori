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
                            request.gate_seconds,
                            request.period_seconds,
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
                            gate_seconds: Some(request.gate_seconds),
                            period_seconds: request.period_seconds,
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
                    gate_seconds: None,
                    period_seconds: None,
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
    stop: watch::Receiver<bool>,
) -> (u64, bool, Result<()>) {
    match request.mode {
        MeasurementMode::SingleLog => {
            run_single_log(config, storage, live, session_id, request, stop).await
        }
        MeasurementMode::SingleDirect => {
            run_single_direct(config, storage, live, session_id, request, stop).await
        }
        MeasurementMode::Multi => run_multi(config, storage, live, session_id, request, stop).await,
    }
}

async fn run_single_log(
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
    let mut connection: Option<Instrument> = None;
    let mut storage_health = storage.health();
    let sample_period = request.period_seconds.unwrap_or(request.gate_seconds);
    let sample_period_ns = seconds_to_ns(sample_period);

    loop {
        match check_stop_or_storage(&stop, &storage_health) {
            LoopState::Continue => {}
            LoopState::Stop => {
                if let Some(instrument) = connection.as_mut() {
                    let _ = instrument.send(":FRUN 0").await;
                }
                return (sequence, degraded, Ok(()));
            }
            LoopState::StorageFailed(error) => {
                return (sequence, degraded, Err(LaboriError::Instrument(error)));
            }
        }
        if connection.is_none() {
            match Instrument::connect(
                &config,
                MeasurementMode::SingleLog,
                request.gate_seconds,
                request.period_seconds,
            )
            .await
            {
                Ok(instrument) => {
                    connection = Some(instrument);
                    let recovered_after_attempts = reconnect_attempts;
                    reconnect_attempts = 0;
                    if sequence > 0 || recovered_after_attempts > 0 {
                        degraded = true;
                        if sequence > 0 {
                            let expected_sequence =
                                origin.elapsed().as_nanos() as u64 / sample_period_ns.max(1);
                            if expected_sequence > sequence {
                                let missing = expected_sequence - sequence;
                                let message = format!(
                                    "estimated {missing} missing samples while disconnected"
                                );
                                if let Err(error) = record_notice(
                                    &storage, &live, session_id, sequence, "gap", message,
                                ) {
                                    return (sequence, degraded, Err(error));
                                }
                                sequence = expected_sequence;
                            }
                        }
                        if let Err(error) = record_notice(
                            &storage,
                            &live,
                            session_id,
                            sequence,
                            "reconnected",
                            "instrument connection restored".to_string(),
                        ) {
                            return (sequence, degraded, Err(error));
                        }
                    }
                }
                Err(error) => {
                    degraded = true;
                    if let Err(error) = handle_connection_error(
                        SessionRefs {
                            config: &config,
                            storage: &storage,
                            live: &live,
                            session_id,
                        },
                        sequence,
                        &mut reconnect_attempts,
                        error,
                        &mut stop,
                        &mut storage_health,
                    )
                    .await
                    {
                        return (sequence, degraded, error);
                    }
                    continue;
                }
            }
        }

        let read_result = {
            let instrument = match connection.as_mut() {
                Some(instrument) => instrument,
                None => continue,
            };
            select_instrument(
                stop.clone(),
                storage_health.clone(),
                instrument.read_log(config.instrument_timeout()),
            )
            .await
        };

        match read_result {
            InstrumentStep::Stop => {
                if let Some(instrument) = connection.as_mut() {
                    let _ = instrument.send(":FRUN 0").await;
                }
                return (sequence, degraded, Ok(()));
            }
            InstrumentStep::StorageFailed(error) => {
                return (sequence, degraded, Err(LaboriError::Instrument(error)));
            }
            InstrumentStep::Done(Ok(values)) => {
                for value in values {
                    let started_ns = (sequence as i64).saturating_mul(sample_period_ns as i64);
                    let sample = Sample {
                        session_id,
                        sequence: sequence as i64,
                        channel: 0,
                        started_ns,
                        ended_ns: started_ns.saturating_add(sample_period_ns as i64),
                        value,
                    };
                    if let Err(error) = storage.try_sample(sample.clone()) {
                        return (sequence, degraded, Err(error));
                    }
                    let _ = live.send(LiveEvent::Sample { sample });
                    sequence += 1;
                }
            }
            InstrumentStep::Done(Err(error)) => {
                degraded = true;
                connection = None;
                if let Err(error) = record_notice(
                    &storage,
                    &live,
                    session_id,
                    sequence,
                    "measurement_error",
                    error.to_string(),
                ) {
                    return (sequence, degraded, Err(error));
                }
            }
        }
    }
}

async fn run_single_direct(
    config: Config,
    storage: StorageHandle,
    live: broadcast::Sender<LiveEvent>,
    session_id: i64,
    request: StartRequest,
    stop: watch::Receiver<bool>,
) -> (u64, bool, Result<()>) {
    run_direct_loop(config, storage, live, session_id, request, stop, None).await
}

async fn run_multi(
    config: Config,
    storage: StorageHandle,
    live: broadcast::Sender<LiveEvent>,
    session_id: i64,
    request: StartRequest,
    stop: watch::Receiver<bool>,
) -> (u64, bool, Result<()>) {
    let gpio = match GpioBank::new() {
        Ok(gpio) => Some(gpio),
        Err(error) => return (0, false, Err(error)),
    };
    run_direct_loop(config, storage, live, session_id, request, stop, gpio).await
}

async fn run_direct_loop(
    config: Config,
    storage: StorageHandle,
    live: broadcast::Sender<LiveEvent>,
    session_id: i64,
    request: StartRequest,
    mut stop: watch::Receiver<bool>,
    mut gpio: Option<GpioBank>,
) -> (u64, bool, Result<()>) {
    let mut sequence = 0_u64;
    let mut degraded = false;
    let mut reconnect_attempts = 0_u32;
    let origin = Instant::now();
    let mut connection: Option<Instrument> = None;
    let mut storage_health = storage.health();
    let channels = if request.mode == MeasurementMode::Multi {
        request.channels.clone()
    } else {
        vec![0]
    };
    let mut channel_index = 0_usize;

    loop {
        if *stop.borrow() {
            clear_gpio(&mut gpio);
            if let Some(instrument) = connection.as_mut() {
                let _ = instrument.send(":FRUN 0").await;
            }
            return (sequence, degraded, Ok(()));
        }
        if let Some(error) = storage_health.borrow().clone() {
            clear_gpio(&mut gpio);
            return (
                sequence,
                degraded,
                Err(LaboriError::Instrument(format!(
                    "storage writer stopped: {error}"
                ))),
            );
        }
        if connection.is_none() {
            match Instrument::connect(
                &config,
                request.mode,
                request.gate_seconds,
                request.period_seconds,
            )
            .await
            {
                Ok(instrument) => {
                    connection = Some(instrument);
                    let recovered_after_attempts = reconnect_attempts;
                    reconnect_attempts = 0;
                    if sequence > 0 || recovered_after_attempts > 0 {
                        degraded = true;
                        if let Err(error) = record_notice(
                            &storage,
                            &live,
                            session_id,
                            sequence,
                            "reconnected",
                            "instrument connection restored".to_string(),
                        ) {
                            return (sequence, degraded, Err(error));
                        }
                    }
                }
                Err(error) => {
                    degraded = true;
                    if let Err(error) = handle_connection_error(
                        SessionRefs {
                            config: &config,
                            storage: &storage,
                            live: &live,
                            session_id,
                        },
                        sequence,
                        &mut reconnect_attempts,
                        error,
                        &mut stop,
                        &mut storage_health,
                    )
                    .await
                    {
                        clear_gpio(&mut gpio);
                        return (sequence, degraded, error);
                    }
                    continue;
                }
            }
        }

        if let Some(period_seconds) = request.period_seconds {
            match wait_for_sample_slot(
                origin,
                sequence,
                period_seconds,
                &mut stop,
                &mut storage_health,
            )
            .await
            {
                SlotWait::Ready => {}
                SlotWait::Stop => {
                    clear_gpio(&mut gpio);
                    if let Some(instrument) = connection.as_mut() {
                        let _ = instrument.send(":FRUN 0").await;
                    }
                    return (sequence, degraded, Ok(()));
                }
                SlotWait::StorageFailed(error) => {
                    clear_gpio(&mut gpio);
                    return (sequence, degraded, Err(LaboriError::Instrument(error)));
                }
            }
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
        let result = {
            let instrument = match connection.as_mut() {
                Some(instrument) => instrument,
                None => continue,
            };
            select_instrument(
                stop.clone(),
                storage_health.clone(),
                instrument.measure(config.instrument_timeout()),
            )
            .await
        };

        match result {
            InstrumentStep::Stop => {
                clear_gpio(&mut gpio);
                if let Some(instrument) = connection.as_mut() {
                    let _ = instrument.send(":FRUN 0").await;
                }
                return (sequence, degraded, Ok(()));
            }
            InstrumentStep::StorageFailed(error) => {
                clear_gpio(&mut gpio);
                return (sequence, degraded, Err(LaboriError::Instrument(error)));
            }
            InstrumentStep::Done(Ok(value)) => {
                let ended_ns = elapsed_ns(origin);
                clear_gpio(&mut gpio);
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
            InstrumentStep::Done(Err(error)) => {
                clear_gpio(&mut gpio);
                degraded = true;
                connection = None;
                if let Err(error) = record_notice(
                    &storage,
                    &live,
                    session_id,
                    sequence,
                    "measurement_error",
                    error.to_string(),
                ) {
                    return (sequence, degraded, Err(error));
                }
            }
        }
    }
}

#[derive(PartialEq, Eq)]
enum LoopState {
    Continue,
    Stop,
    StorageFailed(String),
}

fn check_stop_or_storage(
    stop: &watch::Receiver<bool>,
    storage_health: &watch::Receiver<Option<String>>,
) -> LoopState {
    if *stop.borrow() {
        return LoopState::Stop;
    }
    if let Some(error) = storage_health.borrow().clone() {
        return LoopState::StorageFailed(error);
    }
    LoopState::Continue
}

enum InstrumentStep<T> {
    Done(Result<T>),
    Stop,
    StorageFailed(String),
}

async fn select_instrument<T>(
    mut stop: watch::Receiver<bool>,
    mut storage_health: watch::Receiver<Option<String>>,
    future: impl std::future::Future<Output = Result<T>>,
) -> InstrumentStep<T> {
    tokio::pin!(future);
    tokio::select! {
        result = &mut future => InstrumentStep::Done(result),
        changed = stop.changed() => {
            let _ = changed;
            InstrumentStep::Stop
        }
        changed = storage_health.changed() => {
            if changed.is_err() {
                InstrumentStep::StorageFailed("storage health channel closed".to_string())
            } else {
                InstrumentStep::StorageFailed(
                    storage_health.borrow().clone().unwrap_or_else(|| "storage writer stopped".to_string())
                )
            }
        }
    }
}

enum SlotWait {
    Ready,
    Stop,
    StorageFailed(String),
}

async fn wait_for_sample_slot(
    origin: Instant,
    sequence: u64,
    period_seconds: f64,
    stop: &mut watch::Receiver<bool>,
    storage_health: &mut watch::Receiver<Option<String>>,
) -> SlotWait {
    let target =
        origin + Duration::from_nanos(sequence.saturating_mul(seconds_to_ns(period_seconds)));
    let now = Instant::now();
    if now >= target {
        return SlotWait::Ready;
    }
    tokio::select! {
        _ = tokio::time::sleep_until(target.into()) => SlotWait::Ready,
        changed = stop.changed() => {
            let _ = changed;
            SlotWait::Stop
        }
        changed = storage_health.changed() => {
            if changed.is_err() {
                SlotWait::StorageFailed("storage health channel closed".to_string())
            } else {
                SlotWait::StorageFailed(
                    storage_health.borrow().clone().unwrap_or_else(|| "storage writer stopped".to_string())
                )
            }
        }
    }
}

struct SessionRefs<'a> {
    config: &'a Config,
    storage: &'a StorageHandle,
    live: &'a broadcast::Sender<LiveEvent>,
    session_id: i64,
}

async fn handle_connection_error(
    refs: SessionRefs<'_>,
    sequence: u64,
    reconnect_attempts: &mut u32,
    error: LaboriError,
    stop: &mut watch::Receiver<bool>,
    storage_health: &mut watch::Receiver<Option<String>>,
) -> std::result::Result<(), Result<()>> {
    *reconnect_attempts = reconnect_attempts.saturating_add(1);
    let message = format!("{error} (reconnect attempt {reconnect_attempts})");
    if *reconnect_attempts == 1 || reconnect_attempts.is_power_of_two() {
        record_notice(
            refs.storage,
            refs.live,
            refs.session_id,
            sequence,
            "connection_error",
            message,
        )
        .map_err(Err)?;
    }
    let exponent = reconnect_attempts.saturating_sub(1).min(10);
    let delay = refs
        .config
        .reconnect_millis
        .saturating_mul(1_u64 << exponent)
        .min(30_000);
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(delay)) => Ok(()),
        changed = stop.changed() => {
            if changed.is_err() || *stop.borrow() {
                Err(Ok(()))
            } else {
                Ok(())
            }
        }
        changed = storage_health.changed() => {
            if changed.is_err() {
                Err(Err(LaboriError::ChannelClosed("storage health")))
            } else {
                Err(Err(LaboriError::Instrument(
                    storage_health.borrow().clone().unwrap_or_else(|| "storage writer stopped".to_string())
                )))
            }
        }
    }
}

fn record_notice(
    storage: &StorageHandle,
    live: &broadcast::Sender<LiveEvent>,
    session_id: i64,
    sequence: u64,
    kind: &'static str,
    message: String,
) -> Result<()> {
    storage.try_event(session_id, sequence as i64, kind, message.clone())?;
    let _ = live.send(LiveEvent::Notice {
        session_id,
        at_sequence: sequence as i64,
        message,
    });
    Ok(())
}

fn clear_gpio(gpio: &mut Option<GpioBank>) {
    if let Some(gpio) = gpio.as_mut() {
        gpio.clear();
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
        gate_seconds: f64,
        period_seconds: Option<f64>,
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
        instrument.send(":GATE:TYPE INT").await?;
        instrument
            .send(&format!(":GATE:TIME {}", gate_value(gate_seconds)))
            .await?;
        let actual_gate_seconds = instrument
            .query(":GATE:TIME?", config.instrument_timeout())
            .await?
            .trim()
            .parse::<f64>()
            .map_err(|_| LaboriError::Instrument("invalid GATE:TIME? response".into()))?;
        let tolerance = gate_seconds.abs().max(1.0) * 1e-9;
        if (actual_gate_seconds - gate_seconds).abs() > tolerance {
            return Err(LaboriError::Instrument(format!(
                "gate time verification failed: requested {gate_seconds}, got {actual_gate_seconds}"
            )));
        }
        match mode {
            MeasurementMode::SingleLog => {
                instrument.send(":FRUN 0").await?;
                if let Some(period_seconds) = period_seconds {
                    instrument
                        .send(&format!(":DISP:SRAT {}", period_value(period_seconds)))
                        .await?;
                    let actual_period_seconds = instrument
                        .query(":DISP:SRAT?", config.instrument_timeout())
                        .await?
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| {
                            LaboriError::Instrument("invalid DISP:SRAT? response".into())
                        })?;
                    let tolerance = period_seconds.abs().max(1.0) * 1e-6;
                    if (actual_period_seconds - period_seconds).abs() > tolerance {
                        return Err(LaboriError::Instrument(format!(
                            "sample period verification failed: requested {period_seconds}, got {actual_period_seconds}"
                        )));
                    }
                }
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

fn period_value(seconds: f64) -> String {
    format!("{seconds:.6}")
}

fn seconds_to_ns(seconds: f64) -> u64 {
    (seconds * 1_000_000_000.0).round().max(1.0) as u64
}

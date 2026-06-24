const MAX_LIVE_POINTS = 10000;
const PAGE_SIZE = 50000;
const MONITOR_VIEW = document.getElementById("monitor");
const HISTORY_VIEW = document.getElementById("history");
const indicator = document.getElementById("socket");
const measurementMode = document.getElementById("measurement_mode");
const gateSelect = document.getElementById("gate_select");
const periodSelect = document.getElementById("period_select");
const sessionTitle = document.getElementById("session_title");
const sampleName = document.getElementById("sample_name");
const sessionNote = document.getElementById("session_note");
const runButton = document.getElementById("run");
const stopButton = document.getElementById("stop");
const historySelect = document.getElementById("history_select");
const saveButton = document.getElementById("save_csv_history");
const removeButton = document.getElementById("remove");
const responseField = document.getElementById("response_field");
const displayMode = document.getElementById("display_mode");
const setBaselineButton = document.getElementById("set_baseline");
const clearBaselineButton = document.getElementById("clear_baseline");
const eventKind = document.getElementById("event_kind");
const eventMessage = document.getElementById("event_message");
const addEventButton = document.getElementById("add_event");
const statsBox = document.getElementById("stats");
const eventList = document.getElementById("event_list");

let connected = false;
let running = false;
let busy = false;
let currentSessionId = null;
let currentSession = null;
let liveSamples = [];
let historySamples = [];
let historyEvents = [];
let baselineFrequency = null;
let latestSequence = -1;

const monitorLayout = {
  title: "QCM monitor",
  xaxis: { title: "time / sec", automargin: true },
  yaxis: { title: "frequency / Hz", automargin: true, tickformat: ".2f" },
  margin: { t: 64 },
};
const historyLayout = {
  title: "QCM data viewer",
  xaxis: { title: "time / sec", automargin: true },
  yaxis: { title: "frequency / Hz", automargin: true, tickformat: ".2f" },
  margin: { t: 64 },
};
const plotConfig = { responsive: true };

Plotly.newPlot(MONITOR_VIEW, [{ x: [], y: [], mode: "lines" }], monitorLayout, plotConfig);
Plotly.newPlot(HISTORY_VIEW, [{ x: [], y: [], mode: "lines" }], historyLayout, plotConfig);

function showResponse(value) {
  responseField.value = typeof value === "string" ? value : JSON.stringify(value);
}

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...(options.headers ?? {}) },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `${response.status} ${response.statusText}`);
  return body;
}

function updateControls() {
  runButton.disabled = !connected || running || busy;
  stopButton.disabled = !connected || !running || busy;
  measurementMode.disabled = !connected || running || busy;
  gateSelect.disabled = !connected || running || busy;
  periodSelect.disabled = !connected || running || busy;
  sessionTitle.disabled = !connected || running || busy;
  sampleName.disabled = !connected || running || busy;
  sessionNote.disabled = !connected || running || busy;
  historySelect.disabled = busy;
  saveButton.disabled = busy || !historySelect.value;
  removeButton.disabled = busy || running || !historySelect.value;
  addEventButton.disabled = busy || !currentSessionId;
  setBaselineButton.disabled = busy || visibleSamples().length === 0;
  clearBaselineButton.disabled = busy || baselineFrequency === null;
}

function setBusy(value) {
  busy = value;
  updateControls();
}

function applyStatus(status) {
  running = Boolean(status.running);
  currentSessionId = status.session_id;
  if (status.title !== undefined) currentSession = status;
  if (status.mode === "single_log" || status.mode === "single_direct") {
    measurementMode.value = status.mode;
  }
  if (status.last_error) showResponse(status.last_error);
  updateControls();
}

function resetMonitor() {
  liveSamples = [];
  latestSequence = -1;
  baselineFrequency = null;
  Plotly.react(MONITOR_VIEW, [{ x: [], y: [], mode: "lines" }], monitorLayout, plotConfig);
  renderStats();
  renderEvents([]);
}

function visibleSamples() {
  return historySamples.length > 0 ? historySamples : liveSamples;
}

function yValue(sample) {
  return displayMode.value === "delta" && baselineFrequency !== null
    ? sample.value - baselineFrequency
    : sample.value;
}

function yTitle() {
  return displayMode.value === "delta" ? "Δf / Hz" : "frequency / Hz";
}

function renderPlot(view, samples, layout) {
  const nextLayout = { ...layout, yaxis: { ...layout.yaxis, title: yTitle() } };
  Plotly.react(
    view,
    [{ x: samples.map(sample => sample.ended_ns / 1e9), y: samples.map(yValue), mode: "lines" }],
    nextLayout,
    plotConfig
  );
}

function mean(values) {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function stddev(values) {
  if (values.length < 2) return 0;
  const average = mean(values);
  return Math.sqrt(mean(values.map(value => (value - average) ** 2)));
}

function driftHzPerMin(samples) {
  if (samples.length < 2) return 0;
  const xs = samples.map(sample => sample.ended_ns / 1e9 / 60);
  const ys = samples.map(sample => yValue(sample));
  const xMean = mean(xs);
  const yMean = mean(ys);
  const numerator = xs.reduce((sum, x, index) => sum + (x - xMean) * (ys[index] - yMean), 0);
  const denominator = xs.reduce((sum, x) => sum + (x - xMean) ** 2, 0);
  return denominator === 0 ? 0 : numerator / denominator;
}

function renderStats() {
  const samples = visibleSamples();
  updateControls();
  if (samples.length === 0) {
    statsBox.textContent = "No data loaded.";
    return;
  }
  const values = samples.map(yValue);
  const latest = samples.at(-1);
  const elapsed = (latest.ended_ns - samples[0].ended_ns) / 1e9;
  statsBox.textContent =
    `n=${samples.length}, elapsed=${elapsed.toFixed(1)} s, ` +
    `latest=${yValue(latest).toFixed(3)} Hz, ` +
    `mean=${mean(values).toFixed(3)} Hz, sd=${stddev(values).toFixed(3)} Hz, ` +
    `drift=${driftHzPerMin(samples).toFixed(4)} Hz/min` +
    (baselineFrequency === null ? "" : `, baseline=${baselineFrequency.toFixed(3)} Hz`);
}

function renderEvents(events) {
  eventList.replaceChildren();
  for (const event of events) {
    const row = document.createElement("div");
    row.className = "event_row";
    row.textContent = `${event.created_at} [${event.kind}] ${event.message}`;
    eventList.append(row);
  }
}

async function refreshSessions() {
  const sessions = await api("/api/sessions?mode=single");
  const selected = historySelect.value;
  historySelect.replaceChildren();
  for (const session of sessions) {
    const option = document.createElement("option");
    option.value = String(session.id);
    const method = session.mode === "single_log" ? "LOG" : "MEAS";
    const label = session.title || session.sample_name || `session ${session.id}`;
    option.textContent =
      `#${session.id} ${label} ${session.started_at} [${method}] [${session.state}]`;
    option.dataset.filename = `labori-${session.id}-${session.started_at.replaceAll(":", "-")}`;
    option.dataset.session = JSON.stringify(session);
    historySelect.append(option);
  }
  if ([...historySelect.options].some(option => option.value === selected)) {
    historySelect.value = selected;
  }
  updateControls();
}

async function readAllSamples(sessionId) {
  const rows = [];
  let after = -1;
  while (true) {
    const page = await api(
      `/api/sessions/${sessionId}/samples?after_sequence=${after}&limit=${PAGE_SIZE}`
    );
    rows.push(...page);
    if (page.length < PAGE_SIZE) return rows;
    after = page.at(-1).sequence;
  }
}

function connectLive() {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(`${protocol}//${location.host}/api/live`);
  socket.addEventListener("open", () => {
    connected = true;
    indicator.value = "connected";
    updateControls();
  });
  socket.addEventListener("message", event => {
    const message = JSON.parse(event.data);
    if (message.type === "status") {
      const wasRunning = running;
      applyStatus(message.status);
      if (wasRunning && !running) refreshSessions().catch(error => showResponse(error.message));
    } else if (
      message.type === "sample" &&
      message.sample.session_id === currentSessionId
    ) {
      liveSamples.push(message.sample);
      if (liveSamples.length > MAX_LIVE_POINTS) liveSamples.shift();
      latestSequence = message.sample.sequence;
      renderPlot(MONITOR_VIEW, liveSamples, monitorLayout);
      renderStats();
    } else if (message.type === "notice") {
      showResponse(message.message);
    }
  });
  socket.addEventListener("close", () => {
    connected = false;
    indicator.value = "disconnected; reconnecting...";
    updateControls();
    setTimeout(connectLive, 1000);
  });
  socket.addEventListener("error", () => socket.close());
}

historySelect.addEventListener("change", async () => {
  if (!historySelect.value) return;
    setBusy(true);
  try {
    historySamples = await readAllSamples(historySelect.value);
    historyEvents = await api(`/api/sessions/${historySelect.value}/events`);
    currentSession = JSON.parse(historySelect.selectedOptions[0].dataset.session);
    currentSessionId = Number(historySelect.value);
    historyLayout.title = historySelect.selectedOptions[0].textContent;
    renderPlot(HISTORY_VIEW, historySamples, historyLayout);
    renderEvents(historyEvents);
    renderStats();
    showResponse(`Loaded ${historySamples.length} samples`);
  } catch (error) {
    showResponse(error.message);
  } finally {
    setBusy(false);
  }
});

runButton.addEventListener("click", async () => {
  setBusy(true);
  try {
    resetMonitor();
    const status = await api("/api/measurements/start", {
      method: "POST",
      body: JSON.stringify({
        mode: measurementMode.value,
        gate_seconds: Number(gateSelect.value),
        period_seconds: periodSelect.value === "" ? null : Number(periodSelect.value),
        title: sessionTitle.value,
        sample_name: sampleName.value,
        note: sessionNote.value,
      }),
    });
    applyStatus(status);
    currentSession = status;
    showResponse(`Started session #${status.session_id}`);
  } catch (error) {
    showResponse(error.message);
  } finally {
    setBusy(false);
  }
});

stopButton.addEventListener("click", async () => {
  setBusy(true);
  try {
    const status = await api("/api/measurements/stop", { method: "POST" });
    applyStatus(status);
    showResponse("Stop requested");
  } catch (error) {
    showResponse(error.message);
  } finally {
    setBusy(false);
  }
});

saveButton.addEventListener("click", () => {
  if (!historySelect.value) return;
  const rows = ["sequence,start_time_s,end_time_s,frequency"];
  const metadata = [
    ["# labori QCM export"],
    [`# exported_at,${new Date().toISOString()}`],
    [`# session_id,${historySelect.value}`],
    [`# title,${currentSession?.title ?? ""}`],
    [`# sample_name,${currentSession?.sample_name ?? ""}`],
    [`# mode,${currentSession?.mode ?? ""}`],
    [`# gate_seconds,${currentSession?.gate_seconds ?? ""}`],
    [`# period_seconds,${currentSession?.period_seconds ?? ""}`],
    [`# note,${String(currentSession?.note ?? "").replaceAll("\n", " ")}`],
    ["# events"],
    ...historyEvents.map(event => [`# ${event.created_at},${event.kind},${event.at_sequence},${event.message}`]),
    ["# data"],
  ];
  const header = baselineFrequency === null
    ? "sequence,start_time_s,end_time_s,frequency"
    : "sequence,start_time_s,end_time_s,frequency,delta_frequency";
  rows[0] = header;
  for (const sample of historySamples) {
    const base = `${sample.sequence},${sample.started_ns / 1e9},${sample.ended_ns / 1e9},${sample.value}`;
    rows.push(baselineFrequency === null ? base : `${base},${sample.value - baselineFrequency}`);
  }
  const blob = new Blob([`${metadata.map(row => row.join("")).join("\n")}\n${rows.join("\n")}\n`],
    { type: "text/csv;charset=utf-8" });
  const link = document.createElement("a");
  link.download = `${historySelect.selectedOptions[0].dataset.filename}.csv`;
  link.href = URL.createObjectURL(blob);
  link.click();
  URL.revokeObjectURL(link.href);
});

removeButton.addEventListener("click", async () => {
  const sessionId = historySelect.value;
  if (!sessionId || !window.confirm(`Delete session #${sessionId}?`)) return;
  setBusy(true);
  try {
    await api(`/api/sessions/${sessionId}`, { method: "DELETE" });
    historySamples = [];
    historyEvents = [];
    await refreshSessions();
    Plotly.react(HISTORY_VIEW, [{ x: [], y: [], mode: "lines" }], historyLayout, plotConfig);
    renderEvents([]);
    renderStats();
    showResponse(`Deleted session #${sessionId}`);
  } catch (error) {
    showResponse(error.message);
  } finally {
    setBusy(false);
  }
});

displayMode.addEventListener("change", () => {
  renderPlot(MONITOR_VIEW, liveSamples, monitorLayout);
  renderPlot(HISTORY_VIEW, historySamples, historyLayout);
  renderStats();
});

setBaselineButton.addEventListener("click", async () => {
  const samples = visibleSamples();
  if (samples.length === 0) return;
  baselineFrequency = mean(samples.map(sample => sample.value));
  displayMode.value = "delta";
  renderPlot(MONITOR_VIEW, liveSamples, monitorLayout);
  renderPlot(HISTORY_VIEW, historySamples, historyLayout);
  renderStats();
  if (currentSessionId) {
    await api(`/api/sessions/${currentSessionId}/events`, {
      method: "POST",
      body: JSON.stringify({
        kind: "baseline",
        message: `Baseline set to ${baselineFrequency.toFixed(6)} Hz from ${samples.length} samples`,
        at_sequence: latestSequence,
      }),
    }).catch(error => showResponse(error.message));
  }
});

clearBaselineButton.addEventListener("click", () => {
  baselineFrequency = null;
  renderPlot(MONITOR_VIEW, liveSamples, monitorLayout);
  renderPlot(HISTORY_VIEW, historySamples, historyLayout);
  renderStats();
});

addEventButton.addEventListener("click", async () => {
  if (!currentSessionId) return;
  const message = eventMessage.value.trim() || eventKind.value;
  setBusy(true);
  try {
    const event = await api(`/api/sessions/${currentSessionId}/events`, {
      method: "POST",
      body: JSON.stringify({
        kind: eventKind.value,
        message,
        at_sequence: latestSequence,
      }),
    });
    historyEvents.push(event);
    renderEvents(historyEvents);
    eventMessage.value = "";
    showResponse(`Added event: ${event.kind}`);
  } catch (error) {
    showResponse(error.message);
  } finally {
    setBusy(false);
  }
});

Promise.all([api("/api/status"), refreshSessions()])
  .then(([status]) => applyStatus(status))
  .catch(error => showResponse(error.message));
connectLive();
updateControls();

const MAX_LIVE_POINTS = 10000;
const PAGE_SIZE = 50000;
const CHANNEL_COUNT = 6;
const monitorViews = Array.from({ length: CHANNEL_COUNT }, (_, index) =>
  document.getElementById(`monitor${index}`)
);
const historyViews = Array.from({ length: CHANNEL_COUNT }, (_, index) =>
  document.getElementById(`history${index}`)
);
const indicator = document.getElementById("socket");
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
const channelInputs = [...document.querySelectorAll('input[name="channel"]')];
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
let historySamples = [];
let historyEvents = [];
let latestSequence = -1;

const monitorLayouts = monitorViews.map((_, channel) => ({
  title: `QCM monitor - CH${channel}`,
  xaxis: { title: "time / sec", automargin: true },
  yaxis: { title: "frequency / Hz", automargin: true, tickformat: ".2f" },
  margin: { t: 64 },
}));
const historyLayouts = historyViews.map((_, channel) => ({
  title: `QCM data viewer - CH${channel}`,
  xaxis: { title: "time / sec", automargin: true },
  yaxis: { title: "frequency / Hz", automargin: true, tickformat: ".2f" },
  margin: { t: 64 },
}));
const plotConfig = { responsive: true };

for (let channel = 0; channel < CHANNEL_COUNT; channel += 1) {
  Plotly.newPlot(monitorViews[channel], [{ x: [], y: [], mode: "lines" }],
    monitorLayouts[channel], plotConfig);
  Plotly.newPlot(historyViews[channel], [{ x: [], y: [], mode: "lines" }],
    historyLayouts[channel], plotConfig);
}

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

function selectedChannels() {
  return channelInputs.filter(input => input.checked).map(input => Number(input.value));
}

function updateControls() {
  runButton.disabled = !connected || running || busy || selectedChannels().length === 0;
  stopButton.disabled = !connected || !running || busy;
  gateSelect.disabled = !connected || running || busy;
  periodSelect.disabled = !connected || running || busy;
  sessionTitle.disabled = !connected || running || busy;
  sampleName.disabled = !connected || running || busy;
  sessionNote.disabled = !connected || running || busy;
  channelInputs.forEach(input => { input.disabled = running || busy; });
  historySelect.disabled = busy;
  saveButton.disabled = busy || !historySelect.value;
  removeButton.disabled = busy || running || !historySelect.value;
  addEventButton.disabled = busy || !currentSessionId;
}

function setBusy(value) {
  busy = value;
  updateControls();
}

function applyStatus(status) {
  running = Boolean(status.running);
  currentSessionId = status.session_id;
  if (status.title !== undefined) currentSession = status;
  if (status.last_error) showResponse(status.last_error);
  updateControls();
}

function resetMonitors() {
  latestSequence = -1;
  monitorViews.forEach((view, channel) => {
    Plotly.react(view, [{ x: [], y: [], mode: "lines" }],
      monitorLayouts[channel], plotConfig);
  });
  renderStats([]);
}

async function refreshSessions() {
  const sessions = await api("/api/sessions?mode=multi");
  const selected = historySelect.value;
  historySelect.replaceChildren();
  for (const session of sessions) {
    const option = document.createElement("option");
    option.value = String(session.id);
    const label = session.title || session.sample_name || `session ${session.id}`;
    option.textContent = `#${session.id} ${label} ${session.started_at} CH[${session.channels}] [${session.state}]`;
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

function renderHistory() {
  for (let channel = 0; channel < CHANNEL_COUNT; channel += 1) {
    const rows = historySamples.filter(sample => sample.channel === channel);
    historyLayouts[channel].title =
      `${historySelect.selectedOptions[0]?.textContent ?? "history"} - CH${channel}`;
    Plotly.react(
      historyViews[channel],
      [{
        x: rows.map(sample => sample.ended_ns / 1e9),
        y: rows.map(sample => sample.value),
        mode: "lines",
      }],
      historyLayouts[channel],
      plotConfig
    );
  }
}

function renderStats(samples = historySamples) {
  if (samples.length === 0) {
    statsBox.textContent = "No data loaded.";
    return;
  }
  const byChannel = Array.from({ length: CHANNEL_COUNT }, (_, channel) =>
    samples.filter(sample => sample.channel === channel)
  );
  statsBox.textContent = byChannel
    .map((rows, channel) => rows.length === 0
      ? `CH${channel}: n=0`
      : `CH${channel}: n=${rows.length}, latest=${rows.at(-1).value.toFixed(3)} Hz`)
    .join(" | ");
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
      const sample = message.sample;
      latestSequence = sample.sequence;
      Plotly.extendTraces(
        monitorViews[sample.channel],
        { x: [[sample.ended_ns / 1e9]], y: [[sample.value]] },
        [0],
        MAX_LIVE_POINTS
      );
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

channelInputs.forEach(input => input.addEventListener("change", updateControls));

historySelect.addEventListener("change", async () => {
  if (!historySelect.value) return;
  setBusy(true);
  try {
    historySamples = await readAllSamples(historySelect.value);
    historyEvents = await api(`/api/sessions/${historySelect.value}/events`);
    currentSession = JSON.parse(historySelect.selectedOptions[0].dataset.session);
    currentSessionId = Number(historySelect.value);
    renderHistory();
    renderEvents(historyEvents);
    renderStats(historySamples);
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
    resetMonitors();
    const status = await api("/api/measurements/start", {
      method: "POST",
      body: JSON.stringify({
        mode: "multi",
        gate_seconds: Number(gateSelect.value),
        period_seconds: periodSelect.value === "" ? null : Number(periodSelect.value),
        title: sessionTitle.value,
        sample_name: sampleName.value,
        note: sessionNote.value,
        channels: selectedChannels(),
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
  const metadata = [
    "# labori QCM export",
    `# exported_at,${new Date().toISOString()}`,
    `# session_id,${historySelect.value}`,
    `# title,${currentSession?.title ?? ""}`,
    `# sample_name,${currentSession?.sample_name ?? ""}`,
    `# mode,${currentSession?.mode ?? ""}`,
    `# gate_seconds,${currentSession?.gate_seconds ?? ""}`,
    `# period_seconds,${currentSession?.period_seconds ?? ""}`,
    `# channels,${currentSession?.channels ?? ""}`,
    `# note,${String(currentSession?.note ?? "").replaceAll("\n", " ")}`,
    "# events",
    ...historyEvents.map(event => `# ${event.created_at},${event.kind},${event.at_sequence},${event.message}`),
    "# data",
  ];
  for (let channel = 0; channel < CHANNEL_COUNT; channel += 1) {
    const samples = historySamples.filter(sample => sample.channel === channel);
    if (samples.length === 0) continue;
    const rows = ["sequence,start_time_s,end_time_s,frequency"];
    for (const sample of samples) {
      rows.push(
        `${sample.sequence},${sample.started_ns / 1e9},${sample.ended_ns / 1e9},${sample.value}`
      );
    }
    const blob = new Blob([`${metadata.join("\n")}\n${rows.join("\n")}\n`],
      { type: "text/csv;charset=utf-8" });
    const link = document.createElement("a");
    link.download = `${historySelect.selectedOptions[0].dataset.filename}-ch${channel}.csv`;
    link.href = URL.createObjectURL(blob);
    link.click();
    URL.revokeObjectURL(link.href);
  }
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
    renderHistory();
    renderEvents([]);
    renderStats([]);
    showResponse(`Deleted session #${sessionId}`);
  } catch (error) {
    showResponse(error.message);
  } finally {
    setBusy(false);
  }
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

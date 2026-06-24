const CHANNEL_COLORS = ["#2066d1", "#dc2626", "#16a34a", "#9333ea", "#ea580c", "#0891b2"];
const MAX_LIVE_POINTS = 20000;
const PAGE_SIZE = 50000;

const $ = id => document.getElementById(id);

const state = {
  connected: false,
  running: false,
  busy: false,
  status: null,
  sessions: [],
  selectedSession: null,
  currentSessionId: null,
  liveSamples: [],
  analysisSamples: [],
  events: [],
  latestSequence: -1,
  latestValue: null,
  baselineFrequency: null,
  liveDisplay: "frequency",
  analysisDisplay: "frequency",
  livePlot: null,
  analysisPlot: null,
  pendingLiveRender: false,
};

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...(options.headers ?? {}) },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `${response.status} ${response.statusText}`);
  return body;
}

function setBusy(value) {
  state.busy = value;
  renderChrome();
}

function badge(element, text, tone = "neutral") {
  element.textContent = text;
  element.className = `badge ${tone}`;
}

function switchView(name) {
  document.querySelectorAll(".view").forEach(view => view.classList.remove("active"));
  document.querySelectorAll(".tab").forEach(tab => tab.classList.toggle("active", tab.dataset.view === name));
  $(`view_${name}`).classList.add("active");
  if (name === "data") refreshSessions().catch(showError);
  if (name === "analysis") renderAnalysis();
}

function showError(error) {
  console.error(error);
  badge($("run_badge"), error.message ?? String(error), "bad");
}

function renderChrome() {
  badge($("connection_badge"), state.connected ? "connected" : "disconnected", state.connected ? "good" : "bad");
  badge($("run_badge"), state.running ? "recording" : "idle", state.running ? "good" : "neutral");
  badge(
    $("session_badge"),
    state.currentSessionId ? `session #${state.currentSessionId}` : "no session",
    state.currentSessionId ? "good" : "neutral",
  );

  $("dash_connection").textContent = state.connected ? "connected" : "disconnected";
  $("dash_running").textContent = state.running ? "recording" : "idle";
  $("dash_session").textContent = state.currentSessionId ? `#${state.currentSessionId}` : "-";
  $("dash_latest").textContent = state.latestValue === null ? "-" : `${state.latestValue.toFixed(3)} Hz`;

  $("start_button").disabled = !state.connected || state.running || state.busy;
  $("stop_button").disabled = !state.connected || !state.running || state.busy;
  $("add_event").disabled = state.busy || !state.currentSessionId;
  $("set_baseline").disabled = visibleSamples().length === 0 || state.busy;
  $("clear_baseline").disabled = state.baselineFrequency === null || state.busy;
  $("load_analysis").disabled = !state.selectedSession || state.busy;
  $("export_csv").disabled = !state.selectedSession || state.analysisSamples.length === 0 || state.busy;
  $("delete_session").disabled = !state.selectedSession || state.running || state.busy;

  const mode = $("measurement_mode").value;
  $("channel_fieldset").style.display = mode === "multi" ? "grid" : "none";
  $("mode_hint").textContent = mode === "single_direct"
    ? ":MEAS?を1回ずつ起動します。period未指定なら最短周期です。長時間の実時間軸を重視する標準モードです。"
    : mode === "single_log"
      ? "装置のフリーランと内蔵ログを使います。periodは:DISP:SRATとして装置側周期に使われます。"
      : "GPIOでチャンネルを切り替え、各サンプルごとに:MEAS?を実行します。";
}

function sessionLabel(session) {
  return session.title || session.sample_name || `session ${session.id}`;
}

async function refreshSessions() {
  const mode = $("session_filter").value;
  state.sessions = await api(`/api/sessions${mode ? `?mode=${encodeURIComponent(mode)}` : ""}`);
  renderSessions();
}

function renderRecentSessions() {
  const container = $("recent_sessions");
  container.replaceChildren();
  for (const session of state.sessions.slice(0, 6)) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "session_item";
    row.textContent = `#${session.id} ${sessionLabel(session)} - ${session.state}`;
    row.addEventListener("click", () => selectSession(session.id, true));
    container.append(row);
  }
}

function renderSessions() {
  const query = $("session_search").value.trim().toLowerCase();
  const rows = state.sessions.filter(session =>
    `${session.id} ${session.title} ${session.sample_name} ${session.note} ${session.state} ${session.mode}`
      .toLowerCase()
      .includes(query)
  );
  const body = $("sessions_table_body");
  body.replaceChildren();
  for (const session of rows) {
    const tr = document.createElement("tr");
    tr.className = state.selectedSession?.id === session.id ? "selected" : "";
    tr.innerHTML = `
      <td>${session.id}</td>
      <td>${escapeHtml(sessionLabel(session))}</td>
      <td>${escapeHtml(session.sample_name || "")}</td>
      <td>${escapeHtml(session.started_at)}</td>
      <td>${escapeHtml(session.mode)}</td>
      <td>${escapeHtml(session.state)}</td>
      <td>${session.sample_count}</td>
    `;
    tr.addEventListener("click", () => selectSession(session.id, false));
    body.append(tr);
  }
  renderRecentSessions();
}

async function selectSession(id, openAnalysis) {
  state.selectedSession = state.sessions.find(session => session.id === id) ?? null;
  state.currentSessionId = state.selectedSession?.id ?? state.currentSessionId;
  $("selected_summary").innerHTML = state.selectedSession ? sessionSummaryHtml(state.selectedSession) : "No session selected.";
  renderSessions();
  if (openAnalysis) await loadAnalysis();
  renderChrome();
}

function sessionSummaryHtml(session) {
  return `
    <dl class="kv">
      <dt>ID</dt><dd>#${session.id}</dd>
      <dt>Title</dt><dd>${escapeHtml(sessionLabel(session))}</dd>
      <dt>Sample</dt><dd>${escapeHtml(session.sample_name || "-")}</dd>
      <dt>Started</dt><dd>${escapeHtml(session.started_at)}</dd>
      <dt>Mode</dt><dd>${escapeHtml(session.mode)}</dd>
      <dt>Gate</dt><dd>${session.gate_seconds} s</dd>
      <dt>Period</dt><dd>${session.period_seconds ?? "fastest"}</dd>
      <dt>State</dt><dd>${escapeHtml(session.state)}</dd>
      <dt>Note</dt><dd>${escapeHtml(session.note || "-")}</dd>
    </dl>
  `;
}

function selectedChannels() {
  return [...document.querySelectorAll('input[name="channel"]:checked')].map(input => Number(input.value));
}

function buildNote() {
  const parts = [];
  if ($("operator_name").value.trim()) parts.push(`operator: ${$("operator_name").value.trim()}`);
  if ($("crystal_id").value.trim()) parts.push(`crystal_id: ${$("crystal_id").value.trim()}`);
  if ($("session_note").value.trim()) parts.push($("session_note").value.trim());
  return parts.join("\n");
}

async function startMeasurement(event) {
  event.preventDefault();
  setBusy(true);
  try {
    const mode = $("measurement_mode").value;
    const request = {
      mode,
      gate_seconds: Number($("gate_select").value),
      period_seconds: $("period_select").value === "" ? null : Number($("period_select").value),
      title: $("session_title").value,
      sample_name: $("sample_name").value,
      note: buildNote(),
      channels: mode === "multi" ? selectedChannels() : [],
    };
    const status = await api("/api/measurements/start", {
      method: "POST",
      body: JSON.stringify(request),
    });
    state.status = status;
    state.running = true;
    state.currentSessionId = status.session_id;
    state.liveSamples = [];
    state.events = [];
    state.latestSequence = -1;
    state.latestValue = null;
    state.baselineFrequency = null;
    renderLive();
    renderEvents("live_events", state.events);
    switchView("live");
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

async function stopMeasurement() {
  setBusy(true);
  try {
    const status = await api("/api/measurements/stop", { method: "POST" });
    state.status = status;
    state.running = Boolean(status.running);
    await refreshSessions();
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

function connectLive() {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(`${protocol}//${location.host}/api/live`);
  socket.addEventListener("open", () => {
    state.connected = true;
    renderChrome();
  });
  socket.addEventListener("message", event => {
    const message = JSON.parse(event.data);
    if (message.type === "status") {
      state.status = message.status;
      state.running = Boolean(message.status.running);
      state.currentSessionId = message.status.session_id ?? state.currentSessionId;
      renderChrome();
      if (!state.running) refreshSessions().catch(showError);
    } else if (message.type === "sample" && message.sample.session_id === state.currentSessionId) {
      state.liveSamples.push(message.sample);
      if (state.liveSamples.length > MAX_LIVE_POINTS) state.liveSamples.shift();
      state.latestSequence = message.sample.sequence;
      state.latestValue = message.sample.value;
      scheduleLiveRender();
    } else if (message.type === "notice") {
      state.events.push({
        created_at: new Date().toISOString(),
        kind: "notice",
        message: message.message,
        at_sequence: message.at_sequence,
      });
      renderEvents("live_events", state.events);
    }
  });
  socket.addEventListener("close", () => {
    state.connected = false;
    renderChrome();
    setTimeout(connectLive, 1000);
  });
  socket.addEventListener("error", () => socket.close());
}

function scheduleLiveRender() {
  if (state.pendingLiveRender) return;
  state.pendingLiveRender = true;
  requestAnimationFrame(() => {
    state.pendingLiveRender = false;
    renderLive();
  });
}

function visibleSamples() {
  return state.analysisSamples.length > 0 ? state.analysisSamples : state.liveSamples;
}

function sampleValue(sample, display) {
  if (display === "delta" && state.baselineFrequency !== null) {
    return sample.value - state.baselineFrequency;
  }
  return sample.value;
}

function buildPlotData(samples, display) {
  const channels = [...new Set(samples.map(sample => sample.channel))].sort((a, b) => a - b);
  if (channels.length === 0) return { data: [[0], [null]], labels: ["frequency"], channels: [0] };
  const xs = samples.map(sample => sample.ended_ns / 1e9);
  const data = [xs];
  const labels = [];
  for (const channel of channels) {
    labels.push(`CH${channel}`);
    data.push(samples.map(sample => sample.channel === channel ? sampleValue(sample, display) : null));
  }
  return { data, labels, channels };
}

function renderUplot(holder, plotKey, samples, display, title) {
  const element = $(holder);
  const { data, labels, channels } = buildPlotData(samples, display);
  const yLabel = display === "delta" ? "Δf / Hz" : "frequency / Hz";
  const seriesCount = labels.length + 1;
  const existing = state[plotKey];
  const width = Math.max(320, element.clientWidth || 800);
  const height = Math.max(260, element.clientHeight || 420);

  const opts = {
    title,
    width,
    height,
    scales: { x: { time: false } },
    axes: [
      { label: "time / s" },
      { label: yLabel },
    ],
    series: [
      {},
      ...labels.map((label, index) => ({
        label,
        stroke: CHANNEL_COLORS[channels[index] % CHANNEL_COLORS.length],
        width: 1.5,
        points: { show: false },
        spanGaps: false,
      })),
    ],
    cursor: { drag: { x: true, y: true } },
  };

  if (!existing || existing.series.length !== seriesCount) {
    if (existing) existing.destroy();
    element.replaceChildren();
    state[plotKey] = new uPlot(opts, data, element);
  } else {
    existing.setSize({ width, height });
    existing.setData(data);
  }
}

function renderLive() {
  const title = state.status?.title || state.selectedSession?.title || "Live measurement";
  $("live_title").textContent = title;
  $("live_meta").textContent = liveMetaText();
  renderUplot("live_plot", "livePlot", state.liveSamples, state.liveDisplay, "Live QCM");
  $("live_stats").textContent = statsText(state.liveSamples, state.liveDisplay);
  renderChrome();
}

function liveMetaText() {
  if (!state.currentSessionId) return "No active session";
  const status = state.status;
  return `#${state.currentSessionId} ${status?.mode ?? ""} gate=${status?.gate_seconds ?? "-"}s period=${status?.period_seconds ?? "fastest"}`;
}

function renderAnalysis() {
  const title = state.selectedSession ? sessionLabel(state.selectedSession) : "Analysis";
  $("analysis_title").textContent = title;
  $("analysis_meta").textContent = state.selectedSession
    ? `#${state.selectedSession.id} ${state.selectedSession.mode} - ${state.selectedSession.started_at}`
    : "Select a session in Data Browser.";
  renderUplot("analysis_plot", "analysisPlot", state.analysisSamples, state.analysisDisplay, "Analysis");
  $("analysis_stats").textContent = statsText(state.analysisSamples, state.analysisDisplay);
  renderEvents("analysis_events", state.events);
}

function statsText(samples, display) {
  if (samples.length === 0) return "No data.";
  const values = samples.map(sample => sampleValue(sample, display)).filter(Number.isFinite);
  const latest = samples.at(-1);
  const elapsed = (latest.ended_ns - samples[0].ended_ns) / 1e9;
  return [
    `n = ${samples.length}`,
    `elapsed = ${elapsed.toFixed(1)} s`,
    `latest = ${sampleValue(latest, display).toFixed(3)} Hz`,
    `mean = ${mean(values).toFixed(3)} Hz`,
    `sd = ${stddev(values).toFixed(3)} Hz`,
    `drift = ${driftHzPerMin(samples, display).toFixed(4)} Hz/min`,
    state.baselineFrequency === null ? "baseline = -" : `baseline = ${state.baselineFrequency.toFixed(6)} Hz`,
  ].join("\n");
}

function mean(values) {
  if (values.length === 0) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function stddev(values) {
  if (values.length < 2) return 0;
  const avg = mean(values);
  return Math.sqrt(mean(values.map(value => (value - avg) ** 2)));
}

function driftHzPerMin(samples, display) {
  if (samples.length < 2) return 0;
  const xs = samples.map(sample => sample.ended_ns / 1e9 / 60);
  const ys = samples.map(sample => sampleValue(sample, display));
  const xMean = mean(xs);
  const yMean = mean(ys);
  const numerator = xs.reduce((sum, x, index) => sum + (x - xMean) * (ys[index] - yMean), 0);
  const denominator = xs.reduce((sum, x) => sum + (x - xMean) ** 2, 0);
  return denominator === 0 ? 0 : numerator / denominator;
}

async function addEvent() {
  if (!state.currentSessionId) return;
  setBusy(true);
  try {
    const event = await api(`/api/sessions/${state.currentSessionId}/events`, {
      method: "POST",
      body: JSON.stringify({
        kind: $("event_kind").value,
        message: $("event_message").value.trim() || $("event_kind").value,
        at_sequence: state.latestSequence,
      }),
    });
    state.events.push(event);
    $("event_message").value = "";
    renderEvents("live_events", state.events);
    renderEvents("analysis_events", state.events);
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

async function setBaseline() {
  const samples = visibleSamples();
  if (samples.length === 0) return;
  state.baselineFrequency = mean(samples.map(sample => sample.value));
  state.liveDisplay = "delta";
  state.analysisDisplay = "delta";
  updateSegmented();
  renderLive();
  renderAnalysis();
  if (state.currentSessionId) {
    await api(`/api/sessions/${state.currentSessionId}/events`, {
      method: "POST",
      body: JSON.stringify({
        kind: "baseline",
        message: `baseline = ${state.baselineFrequency.toFixed(6)} Hz from ${samples.length} samples`,
        at_sequence: state.latestSequence,
      }),
    }).catch(showError);
  }
}

function clearBaseline() {
  state.baselineFrequency = null;
  renderLive();
  renderAnalysis();
}

function renderEvents(id, events) {
  const container = $(id);
  container.replaceChildren();
  if (events.length === 0) {
    const empty = document.createElement("div");
    empty.className = "muted";
    empty.textContent = "No events.";
    container.append(empty);
    return;
  }
  for (const event of events) {
    const item = document.createElement("div");
    item.className = "event_item";
    item.textContent = `${event.created_at ?? ""} [${event.kind}] ${event.message}`;
    container.append(item);
  }
}

async function loadAnalysis() {
  if (!state.selectedSession) return;
  setBusy(true);
  try {
    state.analysisSamples = await readAllSamples(state.selectedSession.id);
    state.events = await api(`/api/sessions/${state.selectedSession.id}/events`);
    state.currentSessionId = state.selectedSession.id;
    switchView("analysis");
    renderAnalysis();
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

async function readAllSamples(sessionId) {
  const rows = [];
  let after = -1;
  while (true) {
    const page = await api(`/api/sessions/${sessionId}/samples?after_sequence=${after}&limit=${PAGE_SIZE}`);
    rows.push(...page);
    if (page.length < PAGE_SIZE) return rows;
    after = page.at(-1).sequence;
  }
}

function exportCsv() {
  if (!state.selectedSession || state.analysisSamples.length === 0) return;
  const session = state.selectedSession;
  const header = state.baselineFrequency === null
    ? "sequence,channel,start_time_s,end_time_s,frequency"
    : "sequence,channel,start_time_s,end_time_s,frequency,delta_frequency";
  const lines = [
    "# labori QCM export",
    `# exported_at,${new Date().toISOString()}`,
    `# session_id,${session.id}`,
    `# title,${csvEscape(session.title)}`,
    `# sample_name,${csvEscape(session.sample_name)}`,
    `# mode,${session.mode}`,
    `# gate_seconds,${session.gate_seconds}`,
    `# period_seconds,${session.period_seconds ?? ""}`,
    `# channels,${csvEscape(session.channels)}`,
    `# note,${csvEscape(String(session.note ?? "").replaceAll("\n", " "))}`,
    "# events",
    ...state.events.map(event => `# ${event.created_at},${event.kind},${event.at_sequence},${csvEscape(event.message)}`),
    "# data",
    header,
    ...state.analysisSamples.map(sample => {
      const base = [
        sample.sequence,
        sample.channel,
        sample.started_ns / 1e9,
        sample.ended_ns / 1e9,
        sample.value,
      ];
      if (state.baselineFrequency !== null) base.push(sample.value - state.baselineFrequency);
      return base.join(",");
    }),
  ];
  const blob = new Blob([`${lines.join("\n")}\n`], { type: "text/csv;charset=utf-8" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = `labori-${session.id}-${session.started_at.replaceAll(":", "-")}.csv`;
  link.click();
  URL.revokeObjectURL(link.href);
}

async function deleteSelectedSession() {
  if (!state.selectedSession) return;
  if (!window.confirm(`Delete session #${state.selectedSession.id} (${sessionLabel(state.selectedSession)})?`)) return;
  setBusy(true);
  try {
    await api(`/api/sessions/${state.selectedSession.id}`, { method: "DELETE" });
    state.selectedSession = null;
    state.analysisSamples = [];
    state.events = [];
    $("selected_summary").textContent = "No session selected.";
    await refreshSessions();
    renderAnalysis();
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

function updateSegmented() {
  document.querySelectorAll("[data-display]").forEach(button => {
    button.classList.toggle("active", button.dataset.display === state.liveDisplay);
  });
  document.querySelectorAll("[data-analysis-display]").forEach(button => {
    button.classList.toggle("active", button.dataset.analysisDisplay === state.analysisDisplay);
  });
}

function csvEscape(value) {
  const text = String(value ?? "");
  return /[",\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, ch => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#039;",
  }[ch]));
}

function bindEvents() {
  document.querySelectorAll(".tab").forEach(tab => {
    tab.addEventListener("click", () => switchView(tab.dataset.view));
  });
  $("dashboard_new").addEventListener("click", () => switchView("new"));
  $("dashboard_data").addEventListener("click", () => switchView("data"));
  $("measurement_form").addEventListener("submit", startMeasurement);
  $("stop_button").addEventListener("click", stopMeasurement);
  $("measurement_mode").addEventListener("change", renderChrome);
  $("refresh_sessions").addEventListener("click", () => refreshSessions().catch(showError));
  $("session_search").addEventListener("input", renderSessions);
  $("session_filter").addEventListener("change", () => refreshSessions().catch(showError));
  $("load_analysis").addEventListener("click", () => loadAnalysis().catch(showError));
  $("export_csv").addEventListener("click", exportCsv);
  $("delete_session").addEventListener("click", () => deleteSelectedSession().catch(showError));
  $("add_event").addEventListener("click", () => addEvent().catch(showError));
  $("set_baseline").addEventListener("click", () => setBaseline().catch(showError));
  $("clear_baseline").addEventListener("click", clearBaseline);
  document.querySelectorAll("[data-display]").forEach(button => {
    button.addEventListener("click", () => {
      state.liveDisplay = button.dataset.display;
      updateSegmented();
      renderLive();
    });
  });
  document.querySelectorAll("[data-analysis-display]").forEach(button => {
    button.addEventListener("click", () => {
      state.analysisDisplay = button.dataset.analysisDisplay;
      updateSegmented();
      renderAnalysis();
    });
  });
  window.addEventListener("resize", () => {
    renderLive();
    renderAnalysis();
  });
}

async function init() {
  bindEvents();
  if (new URLSearchParams(location.search).get("mode") === "multi") {
    $("measurement_mode").value = "multi";
  }
  renderChrome();
  renderEvents("live_events", []);
  renderEvents("analysis_events", []);
  connectLive();
  try {
    state.status = await api("/api/status");
    state.running = Boolean(state.status.running);
    state.currentSessionId = state.status.session_id;
    await refreshSessions();
    renderChrome();
  } catch (error) {
    showError(error);
  }
}

init();

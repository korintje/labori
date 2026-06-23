const MAX_LIVE_POINTS = 10000;
const CHANNEL_COUNT = 6;
const ACK_TIMEOUT_MS = 12000;
const monitorViews = Array.from({ length: CHANNEL_COUNT }, (_, index) =>
  document.getElementById(`monitor${index}`)
);
const historyViews = Array.from({ length: CHANNEL_COUNT }, (_, index) =>
  document.getElementById(`history${index}`)
);
const indicator = document.getElementById("socket");
const intervalSelect = document.getElementById("interval_select");
const runButton = document.getElementById("run");
const stopButton = document.getElementById("stop");
const historySelect = document.getElementById("history_select");
const saveButton = document.getElementById("save_csv_history");
const removeButton = document.getElementById("remove");
const responseField = document.getElementById("response_field");
const channelInputs = [...document.querySelectorAll('input[name="channel"]')];

let running = false;
let busy = false;
let historyData = Array.from({ length: CHANNEL_COUNT }, () => [[], []]);

const monitorLayouts = monitorViews.map((_, index) => ({
  title: `QCM monitor - CH${index}`,
  xaxis: { title: "time / sec", automargin: true },
  yaxis: { title: "frequency / Hz", automargin: true, tickformat: ".2f" },
  margin: { t: 64 },
}));
const historyLayouts = historyViews.map((_, index) => ({
  title: `QCM data viewer - CH${index}`,
  xaxis: { title: "time / sec", automargin: true },
  yaxis: { title: "frequency / Hz", automargin: true, tickformat: ".2f" },
  margin: { t: 64 },
}));
const plotConfig = { responsive: true };

for (let index = 0; index < CHANNEL_COUNT; index += 1) {
  Plotly.newPlot(
    monitorViews[index],
    [{ x: [], y: [], mode: "lines" }],
    monitorLayouts[index],
    plotConfig
  );
  Plotly.newPlot(
    historyViews[index],
    [{ x: [], y: [], mode: "lines" }],
    historyLayouts[index],
    plotConfig
  );
}

function showResponse(value) {
  responseField.value = typeof value === "string" ? value : JSON.stringify(value);
}

function isSuccess(response) {
  return Boolean(response && response.Success);
}

function emitWithAck(event, payload, callback) {
  socket.timeout(ACK_TIMEOUT_MS).emit(event, payload, (error, response) => {
    if (error) {
      callback({
        Failure: {
          MachineNotRespond: `${event} timed out after ${ACK_TIMEOUT_MS} ms`,
        },
      });
      return;
    }
    callback(response);
  });
}

function selectedChannels() {
  return channelInputs
    .filter(input => input.checked)
    .map(input => Number(input.value));
}

function updateControls() {
  const connected = socket.connected;
  runButton.disabled =
    !connected || running || busy || selectedChannels().length === 0;
  stopButton.disabled = !connected || !running || busy;
  intervalSelect.disabled = !connected || running || busy;
  channelInputs.forEach(input => {
    input.disabled = running || busy;
  });
  historySelect.disabled = busy;
  saveButton.disabled = busy || historySelect.selectedIndex < 0;
  removeButton.disabled = busy || running || historySelect.selectedIndex < 0;
}

function setBusy(value) {
  busy = value;
  updateControls();
}

function setIntervalOption(value) {
  const option = [...intervalSelect.options].find(item => item.value === value);
  if (option) intervalSelect.value = value;
}

function resetMonitors() {
  for (let index = 0; index < monitorViews.length; index += 1) {
    Plotly.react(
      monitorViews[index],
      [{ x: [], y: [], mode: "lines" }],
      monitorLayouts[index],
      plotConfig
    );
  }
}

function downloadCsv(data, tableName) {
  for (let channel = 0; channel < data.length; channel += 1) {
    const xs = data[channel]?.[0] ?? [];
    const ys = data[channel]?.[1] ?? [];
    if (xs.length === 0 && ys.length === 0) continue;
    const rows = ["time(s),freq(Hz)"];
    for (let index = 0; index < Math.min(xs.length, ys.length); index += 1) {
      rows.push(`${xs[index]},${ys[index]}`);
    }
    const blob = new Blob([`${rows.join("\n")}\n`], {
      type: "text/csv;charset=utf-8",
    });
    const link = document.createElement("a");
    link.download = `${tableName}-ch${channel}.csv`;
    link.href = URL.createObjectURL(blob);
    link.click();
    URL.revokeObjectURL(link.href);
  }
}

const socket = io({
  reconnection: true,
  reconnectionAttempts: Infinity,
  reconnectionDelay: 1000,
});

socket.on("connect", () => {
  indicator.value = "connected";
  showResponse("connected to server");
  updateControls();
  emitWithAck("refresh_tables", "", () => {});
});

socket.on("disconnect", () => {
  indicator.value = "disconnected";
  showResponse("disconnected from server; reconnecting...");
  updateControls();
});

socket.on("connect_error", error => showResponse({ connection_error: error.message }));
socket.on("measurement_state", state => {
  running = Boolean(state.running);
  updateControls();
});
socket.on("update_interval", setIntervalOption);

socket.on("update_table_list", tables => {
  const selected = historySelect.value;
  historySelect.replaceChildren();
  for (const table of tables.filter(item => String(item.channels).includes(","))) {
    const option = document.createElement("option");
    option.value = table.table_name;
    option.textContent = table.table_name;
    historySelect.append(option);
  }
  if ([...historySelect.options].some(option => option.value === selected)) {
    historySelect.value = selected;
  }
  updateControls();
});

socket.on("update_monitor", rows => {
  const updates = Array.from({ length: CHANNEL_COUNT }, () => ({ x: [], y: [] }));
  for (const row of rows) {
    if (
      row.channel >= 0 &&
      row.channel < updates.length &&
      Number.isFinite(row.start_time) &&
      Number.isFinite(row.freq)
    ) {
      updates[row.channel].x.push(row.start_time);
      updates[row.channel].y.push(row.freq);
    }
  }
  updates.forEach((update, channel) => {
    if (update.x.length === 0) return;
    Plotly.extendTraces(
      monitorViews[channel],
      { x: [update.x], y: [update.y] },
      [0],
      MAX_LIVE_POINTS
    );
  });
});

channelInputs.forEach(input => input.addEventListener("change", updateControls));

intervalSelect.addEventListener("change", () => {
  setBusy(true);
  emitWithAck("set_interval", intervalSelect.value, response => {
    showResponse(response);
    setBusy(false);
  });
});

historySelect.addEventListener("change", () => {
  if (!historySelect.value) return;
  setBusy(true);
  emitWithAck("read_db_multi", historySelect.value, data => {
    if (data.error) {
      showResponse(data);
      setBusy(false);
      return;
    }
    historyData = data;
    for (let channel = 0; channel < historyViews.length; channel += 1) {
      historyLayouts[channel].title = `${historySelect.value} - CH${channel}`;
      Plotly.react(
        historyViews[channel],
        [{
          x: historyData[channel]?.[0] ?? [],
          y: historyData[channel]?.[1] ?? [],
          mode: "lines",
        }],
        historyLayouts[channel],
        plotConfig
      );
    }
    showResponse(`Got data from ${historySelect.value}`);
    setBusy(false);
  });
});

runButton.addEventListener("click", () => {
  const channels = selectedChannels();
  if (channels.length === 0) return;
  setBusy(true);
  emitWithAck("set_interval", intervalSelect.value, setResponse => {
    if (!isSuccess(setResponse)) {
      showResponse(setResponse);
      setBusy(false);
      return;
    }
    emitWithAck(
      "run_multi",
      { interval: Number(intervalSelect.value), channels },
      runResponse => {
        showResponse(runResponse);
        if (isSuccess(runResponse)) {
          running = true;
          resetMonitors();
        }
        setBusy(false);
      }
    );
  });
});

stopButton.addEventListener("click", () => {
  setBusy(true);
  emitWithAck("stop", "", response => {
    showResponse(response);
    if (isSuccess(response)) running = false;
    setBusy(false);
  });
});

saveButton.addEventListener("click", () => {
  if (!historySelect.value) return;
  downloadCsv(historyData, historySelect.value.replaceAll(":", "-"));
});

removeButton.addEventListener("click", () => {
  const tableName = historySelect.value;
  if (!tableName || !window.confirm(`Are you sure you want to remove ${tableName}?`)) {
    return;
  }
  setBusy(true);
  emitWithAck("remove", tableName, response => {
    showResponse(response);
    if (response.TableRemoved) {
      historyData = Array.from({ length: CHANNEL_COUNT }, () => [[], []]);
      for (let channel = 0; channel < historyViews.length; channel += 1) {
        historyLayouts[channel].title = `QCM data viewer - CH${channel}`;
        Plotly.react(
          historyViews[channel],
          [{ x: [], y: [], mode: "lines" }],
          historyLayouts[channel],
          plotConfig
        );
      }
    }
    setBusy(false);
  });
});

updateControls();

const MAX_LIVE_POINTS = 10000;
const ACK_TIMEOUT_MS = 12000;
const MONITOR_VIEW = document.getElementById("monitor");
const HISTORY_VIEW = document.getElementById("history");
const indicator = document.getElementById("socket");
const intervalSelect = document.getElementById("interval_select");
const runButton = document.getElementById("run");
const stopButton = document.getElementById("stop");
const historySelect = document.getElementById("history_select");
const saveButton = document.getElementById("save_csv_history");
const removeButton = document.getElementById("remove");
const responseField = document.getElementById("response_field");

let running = false;
let busy = false;
let historyX = [];
let historyY = [];

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

function updateControls() {
  const connected = socket.connected;
  runButton.disabled = !connected || running || busy;
  stopButton.disabled = !connected || !running || busy;
  intervalSelect.disabled = !connected || running || busy;
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

function downloadCsv(xs, ys, tableName) {
  const rows = ["time(sec),frequency(Hz)"];
  for (let index = 0; index < Math.min(xs.length, ys.length); index += 1) {
    rows.push(`${xs[index]},${ys[index]}`);
  }
  const blob = new Blob([`${rows.join("\n")}\n`], { type: "text/csv;charset=utf-8" });
  const link = document.createElement("a");
  link.download = `${tableName}.csv`;
  link.href = URL.createObjectURL(blob);
  link.click();
  URL.revokeObjectURL(link.href);
}

function resetMonitor() {
  Plotly.react(
    MONITOR_VIEW,
    [{ x: [], y: [], mode: "lines" }],
    monitorLayout,
    plotConfig
  );
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
  for (const table of tables.filter(item => !String(item.channels).includes(","))) {
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
  const validRows = rows.filter(row =>
    Number.isFinite(row.time) && Number.isFinite(row.freq)
  );
  if (validRows.length === 0) return;
  Plotly.extendTraces(
    MONITOR_VIEW,
    {
      x: [validRows.map(row => row.time)],
      y: [validRows.map(row => row.freq)],
    },
    [0],
    MAX_LIVE_POINTS
  );
});

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
  emitWithAck("read_db", historySelect.value, data => {
    if (data.error) {
      showResponse(data);
      setBusy(false);
      return;
    }
    historyX = data[0];
    historyY = data[1];
    historyLayout.title = historySelect.value;
    Plotly.react(
      HISTORY_VIEW,
      [{ x: historyX, y: historyY, mode: "lines" }],
      historyLayout,
      plotConfig
    );
    showResponse(`Got data from ${historySelect.value}`);
    setBusy(false);
  });
});

runButton.addEventListener("click", () => {
  setBusy(true);
  emitWithAck("set_interval", intervalSelect.value, setResponse => {
    if (!isSuccess(setResponse)) {
      showResponse(setResponse);
      setBusy(false);
      return;
    }
    emitWithAck("run", intervalSelect.value, runResponse => {
      showResponse(runResponse);
      if (isSuccess(runResponse)) {
        running = true;
        resetMonitor();
      }
      setBusy(false);
    });
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
  downloadCsv(
    historyX,
    historyY,
    historySelect.value.replaceAll(":", "-")
  );
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
      historyX = [];
      historyY = [];
      historyLayout.title = "QCM data viewer";
      Plotly.react(
        HISTORY_VIEW,
        [{ x: [], y: [], mode: "lines" }],
        historyLayout,
        plotConfig
      );
    }
    setBusy(false);
  });
});

updateControls();

const fs = require("fs");
const path = require("path");
const express = require("express");
const net = require("net");
const sqlite3 = require("sqlite3");
const toml = require("toml");

const app = express();
const http = require("http").Server(app);
const io = require("socket.io")(http);

const configPath = path.join(__dirname, "config.toml");
const config = toml.parse(fs.readFileSync(configPath, "utf-8"));
const TCP_HOST = config.TCP_client.address;
const TCP_PORT = config.TCP_client.port;
const TCP_TIMEOUT = config.TCP_client.timeout_ms ?? 10000;
const WS_HOST = config.WS_server.address;
const WS_PORT = config.WS_server.port;
const SAMPLE_RATE = config.database.sampling_rate;
const DB_PATH = path.resolve(__dirname, config.database.path);
const RESERVED_TABLES = new Set(["registry", "sqlite_sequence"]);

const db = new sqlite3.Database(DB_PATH);
let activeTable = null;

app.get("/app/qcm", (_, res) => res.sendFile(path.join(__dirname, "index.html")));
app.use("/app/qcm", express.static(path.join(__dirname, "public")));
app.get("/app/qcm-multi", (_, res) => res.sendFile(path.join(__dirname, "index-multi.html")));
app.use("/app/qcm-multi", express.static(path.join(__dirname, "public")));

http.listen(WS_PORT, WS_HOST, () => {
  console.log(`listening on http://${WS_HOST}:${WS_PORT}`);
  console.log(`database: ${DB_PATH}`);
});

function requestBackend(command, callback) {
  const client = net.connect(TCP_PORT, TCP_HOST);
  let response = "";
  let completed = false;

  const finish = () => {
    if (completed || response.trim() === "") {
      return;
    }
    completed = true;
    try {
      callback(JSON.parse(response));
    } catch (error) {
      callback({ Failure: { InvalidReturn: error.message } });
    }
  };

  client.setEncoding("utf8");
  client.setTimeout(TCP_TIMEOUT);
  client.on("connect", () => client.write(`${command}\n`));
  client.on("data", data => {
    response += data;
    if (response.includes("\n")) {
      finish();
      client.end();
    }
  });
  client.on("end", finish);
  client.on("timeout", () => {
    if (!completed) {
      completed = true;
      callback({
        Failure: {
          MachineNotRespond: `Backend response timed out after ${TCP_TIMEOUT} ms`,
        },
      });
    }
    client.destroy();
  });
  client.on("error", error => {
    if (!completed) {
      completed = true;
      callback({ Failure: { MachineNotRespond: error.message } });
    }
  });
}

function quoteIdentifier(identifier) {
  return `"${String(identifier).replaceAll('"', '""')}"`;
}

function getRegisteredTable(tableName, callback) {
  if (
    typeof tableName !== "string" ||
    tableName.length === 0 ||
    RESERVED_TABLES.has(tableName)
  ) {
    callback(new Error(`Invalid measurement table: ${tableName}`));
    return;
  }
  db.get(
    "SELECT table_name, channels, interval FROM registry WHERE table_name = ?",
    [tableName],
    (registryError, row) => {
      if (row) {
        callback(null, row);
        return;
      }
      db.all(
        `PRAGMA table_info(${quoteIdentifier(tableName)})`,
        (tableError, columns = []) => {
          if (tableError || columns.length === 0) {
            callback(new Error(`Unknown measurement table: ${tableName}`));
            return;
          }
          const columnNames = new Set(columns.map(column => column.name));
          const isSingle =
            columnNames.has("time") &&
            columnNames.has("freq") &&
            columnNames.has("rate");
          const isMulti =
            columnNames.has("channel") &&
            columnNames.has("start_time") &&
            columnNames.has("end_time") &&
            columnNames.has("freq");
          if (!isSingle && !isMulti) {
            callback(new Error(`Invalid measurement table: ${tableName}`));
            return;
          }
          callback(null, {
            table_name: tableName,
            channels: isMulti ? "0,1,2,3" : "0",
            interval: 0,
          });
        }
      );
    }
  );
}

function getTables(socket) {
  db.all(
    "SELECT name FROM sqlite_master " +
      "WHERE type = 'table' AND name NOT IN ('registry', 'sqlite_sequence') " +
      "ORDER BY name DESC",
    (_tableError, tables = []) => {
      db.all(
        "SELECT table_name, channels, interval FROM registry ORDER BY rowid DESC",
        (_registryError, registryRows = []) => {
          const registered = new Map(
            registryRows.map(row => [row.table_name, row])
          );
          const rows = [];
          let pending = tables.length;

          if (pending === 0) {
            socket.emit("update_table_list", rows);
            return;
          }

          for (const { name } of tables) {
            if (registered.has(name)) {
              rows.push(registered.get(name));
              if (--pending === 0) socket.emit("update_table_list", rows);
              continue;
            }

            // Infer the mode for databases created before registry entries existed.
            db.all(`PRAGMA table_info(${quoteIdentifier(name)})`, (_error, columns = []) => {
              const isMulti = columns.some(column => column.name === "channel");
              rows.push({
                table_name: name,
                channels: isMulti ? "0,1,2,3" : "0",
                interval: 0,
              });
              if (--pending === 0) socket.emit("update_table_list", rows);
            });
          }
        }
      );
    }
  );
}

function stream(socket, tableName, state) {
  if (state.queryInFlight) {
    return;
  }
  state.queryInFlight = true;
  const table = quoteIdentifier(tableName);
  db.all(
    `SELECT *, rowid FROM ${table} WHERE rowid > ?`,
    [state.lastRowId],
    (error, rows = []) => {
      state.queryInFlight = false;
      if (error) {
        console.error(`Failed to stream ${tableName}:`, error.message);
        return;
      }
      const lastRow = rows.at(-1);
      if (lastRow) {
        state.lastRowId = lastRow.rowid;
        socket.emit("update_monitor", rows);
      }
    }
  );
}

function startStreaming(socket, tableName, state) {
  clearInterval(state.streaming);
  state.lastRowId = 0;
  state.queryInFlight = false;
  state.streaming = setInterval(
    () => stream(socket, tableName, state),
    SAMPLE_RATE
  );
}

function isTerminalMeasurementFailure(response) {
  return Boolean(response.Failure && (
    response.Failure.MachineNotRespond ||
    response.Failure.PollerCommandNotSent ||
    response.Failure.SaveDataFailed ||
    response.Failure.ErrorInRunning ||
    response.Failure.NotRunning
  ));
}

function checkRun(socket, state) {
  requestBackend(JSON.stringify({ Get: { key: "Interval" } }), response => {
    if (response.Success) {
      activeTable = null;
      socket.emit("measurement_state", { running: false, table_name: null });
      socket.emit("update_interval", response.Success.GotValue);
    } else if (response.Failure && response.Failure.Busy) {
      const busy = response.Failure.Busy;
      activeTable = busy.table_name;
      socket.emit("measurement_state", {
        running: true,
        table_name: busy.table_name,
      });
      socket.emit("update_interval", busy.interval);
      startStreaming(socket, busy.table_name, state);
    } else if (isTerminalMeasurementFailure(response)) {
      activeTable = null;
      socket.emit("measurement_state", { running: false, table_name: null });
    }
  });
}

io.on("connection", socket => {
  const state = { streaming: null, lastRowId: 0, queryInFlight: false };

  checkRun(socket, state);
  getTables(socket);

  socket.on("disconnect", () => {
    clearInterval(state.streaming);
    state.lastRowId = 0;
  });

  socket.on("read_db", (tableName, callback) => {
    getRegisteredTable(tableName, (validationError, metadata) => {
      if (validationError || String(metadata.channels).includes(",")) {
        callback({ error: validationError?.message ?? "Not a single-channel table" });
        return;
      }
      const table = quoteIdentifier(tableName);
      db.all(`SELECT time, freq FROM ${table}`, (error, rows = []) => {
        if (error) {
          callback({ error: error.message });
          return;
        }
        callback([
          rows.map(row => row.time),
          rows.map(row => row.freq),
        ]);
      });
    });
  });

  socket.on("read_db_multi", (tableName, callback) => {
    getRegisteredTable(tableName, (validationError, metadata) => {
      if (validationError || !String(metadata.channels).includes(",")) {
        callback({ error: validationError?.message ?? "Not a multichannel table" });
        return;
      }
      const table = quoteIdentifier(tableName);
      db.all(
        `SELECT channel, start_time, end_time, freq FROM ${table}`,
        (error, rows = []) => {
          if (error) {
            callback({ error: error.message });
            return;
          }
          const channels = Array.from({ length: 6 }, () => [[], []]);
          for (const row of rows) {
            if (channels[row.channel]) {
              channels[row.channel][0].push(row.start_time);
              channels[row.channel][1].push(row.freq);
            }
          }
          callback(channels);
        }
      );
    });
  });

  socket.on("refresh_tables", (_arg, callback) => {
    getTables(socket);
    callback?.({ Success: "Table list refreshed" });
  });

  socket.on("get_interval", (_arg, callback) => {
    requestBackend(JSON.stringify({ Get: { key: "Interval" } }), callback);
  });

  socket.on("set_interval", (interval, callback) => {
    requestBackend(
      JSON.stringify({ Set: { key: "Interval", value: interval } }),
      callback
    );
  });

  socket.on("run", (duration, callback) => {
    requestBackend(
      JSON.stringify({ RunExt: { duration } }),
      response => {
        if (response.Success && response.Success.SaveTable) {
          activeTable = response.Success.SaveTable;
          io.emit("measurement_state", {
            running: true,
            table_name: activeTable,
          });
          startStreaming(socket, response.Success.SaveTable, state);
        }
        callback(response);
      }
    );
  });

  socket.on("run_multi", ({ interval, channels }, callback) => {
    const numericInterval = Number(interval);
    const selectedChannels = Array.isArray(channels)
      ? channels.map(Number).filter(Number.isInteger)
      : [];
    if (
      !Number.isFinite(numericInterval) ||
      numericInterval <= 0 ||
      numericInterval > 10 ||
      selectedChannels.length === 0 ||
      selectedChannels.some(channel => channel < 0 || channel > 5)
    ) {
      callback({ Failure: { InvalidRequest: "Select at least one valid channel" } });
      return;
    }
    requestBackend(
      JSON.stringify({
        RunMulti: {
          channels: [...new Set(selectedChannels)],
          interval: numericInterval,
        },
      }),
      response => {
        if (response.Success && response.Success.SaveTable) {
          activeTable = response.Success.SaveTable;
          io.emit("measurement_state", {
            running: true,
            table_name: activeTable,
          });
          startStreaming(socket, response.Success.SaveTable, state);
        }
        callback(response);
      }
    );
  });

  socket.on("stop", (_arg, callback) => {
    requestBackend(JSON.stringify({ Stop: {} }), response => {
      const terminalFailure = isTerminalMeasurementFailure(response);
      if (response.Success || terminalFailure) {
        activeTable = null;
        io.emit("measurement_state", { running: false, table_name: null });
        clearInterval(state.streaming);
        state.streaming = null;
        state.lastRowId = 0;
        getTables(socket);
      }
      callback(response);
    });
  });

  socket.on("remove", (tableName, callback) => {
    if (tableName === activeTable) {
      callback({
        Failure: {
          Busy: {
            table_name: activeTable,
            interval: "unknown",
          },
        },
      });
      return;
    }
    getRegisteredTable(tableName, validationError => {
      if (validationError) {
        callback({ Failure: { InvalidRequest: validationError.message } });
        return;
      }
      const table = quoteIdentifier(tableName);
      db.serialize(() => {
        db.run(`DROP TABLE IF EXISTS ${table}`, dropError => {
          if (dropError) {
            callback({ Failure: { SaveDataFailed: dropError.message } });
            return;
          }
          db.run("DELETE FROM registry WHERE table_name = ?", [tableName], error => {
            if (error && !error.message.includes("no such table")) {
              callback({ Failure: { SaveDataFailed: error.message } });
              return;
            }
            getTables(socket);
            callback({ TableRemoved: tableName });
          });
        });
      });
    });
  });
});

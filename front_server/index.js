// Import mudules
const app  = require("express")();
const express = require("express");
const http = require("http").Server(app);
const io = require("socket.io")(http);
const sqlite3 = require("sqlite3");
const net = require('net');

// Global constants
TCP_PORT = 50001;
WS_PORT = 3000;
SAMPLE_RATE = 100;

// Database connection
const db = new sqlite3.Database("../back_client/Iwatsu.db");

// Return index.html and static files directory path
app.get('/app/qcm', (_, res) => { res.sendFile(__dirname + '/index.html'); });
app.use('/app/qcm', express.static(__dirname + "/public"));

// Listen websocket port
http.listen(WS_PORT, function(){
  console.log(`listening on *:${WS_PORT}`);
});

// Be a client of TCP server
const connect_TCP = (cmd) => {
  const client = net.connect(TCP_PORT, 'localhost', () => {
    console.log('connected to TCP server');
    client.write(cmd);
  });
  return client;
};

// Pass response from back client to websocket client
const path_through = (callback, cmd) => {
  const client = connect_TCP(cmd);
  client.on('data', data => {
    console.log('Received from TCP server: ' + data);
    callback(JSON.parse(data));
  });  
};

// Check wheather polling process is running
const check_run = (socket, state) => {
  const client = connect_TCP(`{"Get": {"key": "Interval"}}`);
  client.on('data', data => {
    console.log('Received from TCP server: ' + data);
    const json_data = JSON.parse(data);
    if ("Success" in json_data) {
      const interval = json_data["Success"]["GotValue"];
      socket.emit("update_interval", interval);
      // clearInterval(state["streaming"]);
    } else if ("Failure" in json_data) {
      const table_name = json_data["Failure"]["Busy"];
      state["last_rowid"] = 0;
      state["streaming"] = setInterval(
        () => { stream(socket, table_name, state); }, SAMPLE_RATE
      );
    }
  });  
};

//  Get and update interval value
const get_and_update_interval = (socket) => {
  const client = connect_TCP(`{"Get": {"key": "Interval"}}`);
  client.on('data', data => {
    console.log('Received from TCP server: ' + data);
    const json_data = JSON.parse(data);
    if ("Success" in json_data) {
      const interval = json_data["Success"]["GotValue"];
      socket.emit("update_interval", interval);
    }
  }); 
}

// Stream data from given database
const stream = (socket, table_name, state) => {
  db.all(`select *, rowid from '${table_name}' where rowid>${state["last_rowid"]}`, (_e, data) => {
    if (data !== undefined) {
      let last_row = data[data.length - 1];
      if (last_row !== undefined) {
        state["last_rowid"] = last_row["rowid"];
        socket.emit("update_monitor", data);
      }
    }
  });
};

// Get table list from DB
const get_tables = (socket) => {
  db.all("select name from sqlite_master where type='table'", function (_e, tables) {
    socket.emit("update_table_list", tables);
  });
};


// Client connection event
io.on("connection", (socket) => {

  // Streaming state
  let state = {"streaming": () => {}, "last_rowid": 0,}

  // Check run and interval
  console.log("a client connected")
  check_run(socket, state);
  get_tables(socket);

  // Reset last Row ID if client disconnected
  socket.on("disconnect", () => {
    state["last_rowid"] = 0;
    console.log("client disconnected")
  });

  // Read database
  socket.on("read_db", (table, callback)  => {
    console.log(`select * from '${table}'`);
    db.all(`select * from '${table}'`, (_err, data) => {
      if (data !== undefined) {
        console.log(`${data.length} data has sent.`);
        callback(data);
      }
    });
  });

  // Get Interval
  socket.on('get_interval', (_arg, callback) => {
    path_through(callback, `{"Get": {"key": "Interval"}}`);
  });
  
  // Set Interval
  socket.on('set_interval', (interval, callback) => {
    path_through(callback, `{"Set": {"key": "Interval", "value": "${interval}"}}`);
  });
  
  // Run measurement
  socket.on('run', (_arg, callback) => {
    // socket.broadcast.emit('init_monitor');
    const client = connect_TCP(`{"Run": {}}`);
    client.on('data', data => {
      console.log('Received from TCP server: ' + data);
      const json_data = JSON.parse(data);
      if ("Success" in json_data) {
        const table_name = json_data["Success"]["SaveTable"];
        state["last_rowid"] = 0;
        state["streaming"] = setInterval(
          () => { stream(socket, table_name, state); }, SAMPLE_RATE
        );
      } else if ("Failure" in json_data) {
        ;
      }
      callback(json_data);
    });
    get_tables(socket);
  });

  // Stop measurement
  socket.on('stop', (_arg, callback) => {
    const client = connect_TCP(`{"Stop": {}}`);
    client.on('data', data => {
      console.log('Received from TCP server: ' + data);
      const json_data = JSON.parse(data);
      if ("Success" in json_data) {
        state["last_rowid"] = 0;
        clearInterval(state["streaming"]);
        get_and_update_interval(socket);
      } else if ("Failure" in json_data) {
        ;
      }
      callback(json_data);
    });
    get_tables(socket);
  });  

  // Remove table
  socket.on('remove', (table_name, callback) => {
    db.run(`drop table if exists '${table_name}'`);
    get_tables(socket);
    callback(JSON.parse(`{"TableRemoved": "${table_name}"}`));
    console.log(`Table "${table_name}" removed`);
  });

  /*
  // Update state
  socket.on('update', (_arg, callback) => {
    check_run(socket, state);
    get_tables(socket);
    callback(`{"Update": {}}`);
  });
  */ 

});
